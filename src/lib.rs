//! Command bodies and environment access for `herdr-last-tab`. `src/main.rs` owns argument
//! parsing and exit-code mapping; everything here returns `Result<(), String>`, where `Err`
//! carries the exact message `main` puts on stderr before exiting `1`.

pub mod herdr;
pub mod state;

use herdr::{Herdr, HerdrError};
use std::path::Path;

/// Reads a required environment variable. Per FR-010, absent *and* empty are both the failure
/// case — an operator who exports `HERDR_BIN_PATH=` gets the same actionable message as one who
/// never set it at all, rather than the plugin silently trying to run an empty string as a
/// command. The message names the variable, since a generic "environment not set" is exactly
/// the unactionable failure the exit-code contract forbids.
pub fn required_env(name: &str) -> Result<String, String> {
    match std::env::var(name) {
        Ok(value) if !value.is_empty() => Ok(value),
        _ => Err(format!("{name} is required but is not set")),
    }
}

/// Reads `field` from an event payload, preferring `data.<field>` and falling back to a
/// top-level `<field>` (contracts/herdr-cli.md#events) — herdr's own payloads nest under `data`,
/// but matching `herdr-last-workspace`'s fallback means a payload delivered unwrapped still
/// parses. Absent, empty, and malformed JSON are all expected states for an event payload
/// (Principle I), so every failure here is `None` rather than a propagated error.
pub fn event_field(event_json: &str, field: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(event_json).ok()?;
    // Each candidate is narrowed to a string *before* the fallback, not after. Falling back on
    // the presence of `data.<field>` rather than on its usability means a nested `null` — or an
    // object, or a number — shadows a perfectly good top-level field and yields `None`. The
    // fallback exists to tolerate a payload shape this plugin did not expect, so letting an
    // unexpected shape defeat it is the one thing it must not do.
    value
        .get("data")
        .and_then(|data| data.get(field))
        .and_then(serde_json::Value::as_str)
        .or_else(|| value.get(field).and_then(serde_json::Value::as_str))
        .map(str::to_string)
}

/// Reads `workspace_id` from `HERDR_PLUGIN_CONTEXT_JSON`. Every field of herdr's
/// `PluginInvocationContext` is optional, and this plugin uses only this one, as a fallback for
/// when the snapshot reports no `focused_workspace_id` — so malformed or absent context is a
/// quiet `None`, never a failure.
pub fn context_workspace_id(context_json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(context_json).ok()?;
    value.get("workspace_id")?.as_str().map(str::to_string)
}

/// Renders a `HerdrError` as the FR-010 stderr message. `TabNotFound` is not itself a failure —
/// callers that reach a `tab focus` are expected to handle it as a no-op before it gets here —
/// but the match stays exhaustive so a fourth variant added later cannot silently fall through
/// without a message.
fn describe(err: HerdrError) -> String {
    match err {
        HerdrError::TabNotFound => {
            "herdr reported that the target tab no longer exists".to_string()
        }
        HerdrError::Reported(message) => message,
        HerdrError::Unreachable(message) => message,
    }
}

// ---------------------------------------------------------------------------------------------
// Command bodies
//
// Every body below follows the same shape, per data-model.md#state-transitions-per-subcommand:
// take the lock, take exactly one `api snapshot`, repair the whole map against it, apply at
// most one transition of the subcommand's own, persist, and let the lock release when `guard`
// drops. Acquiring the lock before the snapshot (rather than after) is deliberate: it is what
// makes "one snapshot, one consistent view" mean something under concurrency, not just within a
// single process — a second invocation cannot even take its own snapshot until this one has
// fully finished, including its `tab focus` call where there is one (data-model.md#concurrency).
//
// Holding the lock across a herdr round-trip has a consequence worth naming: nothing here bounds
// how long that round-trip may take, so a herdr that accepts the call and then stops responding
// leaves this invocation holding `state.lock` and every later toggle and hook blocking in
// `StateLock::acquire`. The ordering is still correct — moving `focus_tab` outside the lock is
// exactly what breaks alternation, per the argument above — and the exposure is not particular to
// `focus_tab`: the `api snapshot` that opens every subcommand is inside the same lock with the
// same unbounded wait. A deadline therefore belongs to the `Herdr` seam as a whole rather than to
// one call site inside it, and is deliberately not added here.
//
// Persisting happens only on the paths that return `Ok(())`. An `Err` here means herdr itself
// failed in a way FR-010 requires surfacing, and repair's results are simply left for the next
// invocation to redo — repair is idempotent, so nothing is lost by not writing on a failure that
// is about to produce a toast anyway.
// ---------------------------------------------------------------------------------------------

/// The `toggle` action — the transition table at data-model.md#toggle--in-the-focused-workspace-w.
pub fn toggle(
    herdr: &dyn Herdr,
    state_dir: &Path,
    context_json: Option<&str>,
) -> Result<(), String> {
    let guard = state::StateLock::acquire(state_dir).map_err(|err| err.to_string())?;
    let snapshot = herdr.snapshot().map_err(describe)?;
    let mut history = state::load(state_dir);
    history.repair(&snapshot);

    // W: `focused_workspace_id` first. `HERDR_PLUGIN_CONTEXT_JSON`'s `workspace_id` is used
    // only as a fallback, and only once the snapshot itself confirms that workspace still
    // exists — every field of the invocation context is optional in herdr's schema, so it is a
    // hint to cross-check, never a source of truth on its own (research.md).
    let workspace_id = snapshot.focused_workspace_id.clone().or_else(|| {
        context_json.and_then(context_workspace_id).filter(|id| {
            snapshot
                .workspaces
                .iter()
                .any(|workspace| &workspace.workspace_id == id)
        })
    });
    let Some(workspace_id) = workspace_id else {
        // No workspace to act in. Repair's results are still persisted — a no-op still
        // repairs (FR-009) — and nothing else about this invocation had anywhere to go.
        return finish(&guard, &history);
    };

    // A: the incomplete-snapshot row. `focused_tab_id` can be null, or can name a tab herdr
    // does not report inside W — the two fields are independently nullable, and neither
    // implies the other. Calling `observe` with an unusable A would write a tab id that names
    // nothing, so the toggle skips the observation entirely rather than guess: it exits `0`
    // having only repaired, exactly as the second table row requires.
    let a = snapshot.focused_tab_id.clone().filter(|tab_id| {
        snapshot
            .tabs
            .iter()
            .any(|tab| &tab.tab_id == tab_id && tab.workspace_id == workspace_id)
    });
    let Some(a) = a else {
        return finish(&guard, &history);
    };

    // W, A, and W's live tab set all come from this one snapshot read, so A cannot be a tab
    // that closed between two separate reads — there is only one. That is what licenses
    // skipping any revalidation of A below and calling `tab focus` on the resolved target
    // without a retry loop (research.md#one-atomic-read).
    history.observe(&workspace_id, &a, &snapshot);

    let target = history
        .workspaces
        .get(&workspace_id)
        .and_then(|entry| entry.last_tab_id.clone());
    let Some(target) = target else {
        return finish(&guard, &history);
    };

    if target == a {
        // Defensive: observe's own rules make a resolved target equal to A unreachable in
        // practice — invariant 2 (current != last) is enforced by repair before observe ever
        // runs, and every branch of observe that could set last_tab_id sets it to the
        // *previous* current, which by construction differs from the new one. The toggle table
        // names this row explicitly regardless, so it is guarded here rather than assumed.
        clear_target(&mut history, &workspace_id);
        return finish(&guard, &history);
    }

    match herdr.focus_tab(&target) {
        Ok(()) => {
            // The toggle writes its own swap — { current: target, last: A } — rather than
            // waiting for the `tab.focused` hook that will follow
            // (research.md#the-toggle-writes-its-own-swap). FR-003 requires repeated
            // invocations to alternate between exactly two tabs, and a second press can race
            // ahead of that hook; if the toggle deferred the write, the second press would read
            // stale history — { current: A, last: target } again — and jump straight back to
            // where the user already is. This is safe only because of FR-002: the hook that
            // eventually arrives names the tab the workspace now already holds as current, so
            // `observe`'s middle case leaves the entry exactly as this write left it, and the
            // two writers converge instead of racing.
            history.workspaces.insert(
                workspace_id,
                state::WorkspaceHistory {
                    current_tab_id: target,
                    last_tab_id: Some(a),
                },
            );
            finish(&guard, &history)
        }
        Err(HerdrError::TabNotFound) => {
            clear_target(&mut history, &workspace_id);
            finish(&guard, &history)
        }
        Err(other) => Err(describe(other)),
    }
}

/// The `tab.focused` hook — data-model.md#tab-focused--event-carries-w-t.
pub fn tab_focused(
    herdr: &dyn Herdr,
    state_dir: &Path,
    event_json: Option<&str>,
) -> Result<(), String> {
    // Parsed up front so a missing or malformed payload is known before the lock is even
    // acquired, though it changes nothing about the repair pass below: repair always runs, and
    // only the transition — the `observe` call — is conditional on having a usable (W, T).
    let observation = event_json.and_then(|json| {
        let workspace_id = event_field(json, "workspace_id")?;
        let tab_id = event_field(json, "tab_id")?;
        Some((workspace_id, tab_id))
    });

    let guard = state::StateLock::acquire(state_dir).map_err(|err| err.to_string())?;
    let snapshot = herdr.snapshot().map_err(describe)?;
    let mut history = state::load(state_dir);
    history.repair(&snapshot);

    if let Some((workspace_id, tab_id)) = observation {
        // Trusted only if this same snapshot — taken under this same lock — currently reports
        // T as the focused tab of W. A payload naming a tab that has since closed or moved
        // elsewhere is stale and must not create or promote history for it (FR-011); the
        // snapshot's own `focused_workspace_id`/`focused_tab_id` pair is what "currently
        // focused" means here, since `TabEntry` carries no per-tab focus flag of its own.
        let still_focused = snapshot.focused_workspace_id.as_deref() == Some(workspace_id.as_str())
            && snapshot.focused_tab_id.as_deref() == Some(tab_id.as_str());
        if still_focused {
            history.observe(&workspace_id, &tab_id, &snapshot);
        }
    }

    finish(&guard, &history)
}

/// The `tab.closed` hook — data-model.md#tab-closed--event-carries-w-t.
///
/// Applies **no transition of its own**. The closed tab is already absent from the snapshot, so
/// repair alone does everything this hook exists to do: clears it from `last_tab_id`, replaces
/// it as `current_tab_id` with the workspace's live `active_tab_id`, or drops the entry if
/// nothing live remains there. An earlier design gave this hook its own rules that duplicated
/// repair's, and the duplication is exactly what let a stale `last_tab_id` survive beside a
/// freshly replaced `current_tab_id` — each rule fixed the ID it was written for and neither
/// looked at the other (research.md#one-repair-pass-separate-from-the-transition-rules). The
/// event payload's `tab_id` is therefore never read: "which tab closed" is a question the
/// snapshot already answers better, for every closed tab including ones whose events were
/// missed, so there is nothing here for a well-formedness check to gate.
pub fn tab_closed(
    herdr: &dyn Herdr,
    state_dir: &Path,
    _event_json: Option<&str>,
) -> Result<(), String> {
    let guard = state::StateLock::acquire(state_dir).map_err(|err| err.to_string())?;
    let snapshot = herdr.snapshot().map_err(describe)?;
    let mut history = state::load(state_dir);
    history.repair(&snapshot);
    finish(&guard, &history)
}

/// The `workspace.closed` hook — data-model.md#workspace-closed--event-carries-w.
///
/// Also applies no transition of its own, for the same reason as `tab_closed`: a closed
/// workspace is absent from the snapshot's `workspaces`, and repair step 1 drops its entry
/// outright. This hook exists anyway because the constitution's Principle II names close events
/// as a pruning path in their own right, independent of repair, and because acting on the event
/// as soon as it arrives keeps the file small between toggles rather than letting a closed
/// workspace's entry sit until the next unrelated invocation happens to repair it away.
pub fn workspace_closed(
    herdr: &dyn Herdr,
    state_dir: &Path,
    _event_json: Option<&str>,
) -> Result<(), String> {
    let guard = state::StateLock::acquire(state_dir).map_err(|err| err.to_string())?;
    let snapshot = herdr.snapshot().map_err(describe)?;
    let mut history = state::load(state_dir);
    history.repair(&snapshot);
    finish(&guard, &history)
}

fn finish(guard: &state::StateLock, history: &state::PersistedState) -> Result<(), String> {
    state::persist(guard, history).map_err(|err| err.to_string())
}

/// Clears `workspace_id`'s target — the shared tail of the toggle's two "the target is gone"
/// rows (a dead `tab focus` and the `target == A` defensive row).
fn clear_target(history: &mut state::PersistedState, workspace_id: &str) {
    if let Some(entry) = history.workspaces.get_mut(workspace_id) {
        entry.last_tab_id = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::herdr::{Snapshot, TabEntry, WorkspaceEntry};
    use crate::state::{PersistedState, WorkspaceHistory};
    use std::cell::RefCell;

    struct FakeHerdr {
        snapshot: Result<Snapshot, HerdrError>,
        focus_result: Result<(), HerdrError>,
        /// Every `tab_id` passed to `focus_tab`, in call order - what T023's assertions and the
        /// concurrency tests need to see, not just the final outcome.
        focus_calls: RefCell<Vec<String>>,
    }

    impl Herdr for FakeHerdr {
        fn snapshot(&self) -> Result<Snapshot, HerdrError> {
            self.snapshot.clone()
        }

        fn focus_tab(&self, tab_id: &str) -> Result<(), HerdrError> {
            self.focus_calls.borrow_mut().push(tab_id.to_string());
            self.focus_result.clone()
        }
    }

    fn empty_snapshot() -> Snapshot {
        Snapshot {
            focused_workspace_id: None,
            focused_tab_id: None,
            workspaces: Vec::<WorkspaceEntry>::new(),
            tabs: Vec::<TabEntry>::new(),
        }
    }

    fn workspace(id: &str, active_tab: &str) -> WorkspaceEntry {
        WorkspaceEntry {
            workspace_id: id.to_string(),
            active_tab_id: active_tab.to_string(),
        }
    }

    fn tab(id: &str, workspace_id: &str) -> TabEntry {
        TabEntry {
            tab_id: id.to_string(),
            workspace_id: workspace_id.to_string(),
        }
    }

    fn history(current: &str, last: Option<&str>) -> WorkspaceHistory {
        WorkspaceHistory {
            current_tab_id: current.to_string(),
            last_tab_id: last.map(str::to_string),
        }
    }

    /// A fresh, empty state directory unique to the calling test, so parallel `cargo test` runs
    /// never share a `state.json`/`state.lock`.
    fn temp_state_dir(label: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "herdr-last-tab-lib-test-{label}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fake(snapshot: Snapshot, focus_result: Result<(), HerdrError>) -> FakeHerdr {
        FakeHerdr {
            snapshot: Ok(snapshot),
            focus_result,
            focus_calls: RefCell::new(Vec::new()),
        }
    }

    /// Seeds the state file directly (bypassing the lock, since no other process is involved)
    /// so a toggle test can start from an arbitrary prior history. The name comes from
    /// `state::STATE_FILE` rather than a literal, so a rename there cannot leave these tests
    /// seeding a file `state::load` no longer reads — which would pass as a silent no-op.
    fn seed_state(dir: &std::path::Path, state: PersistedState) {
        std::fs::write(
            dir.join(crate::state::STATE_FILE),
            serde_json::to_vec(&state).unwrap(),
        )
        .unwrap();
    }

    fn load_state(dir: &std::path::Path) -> PersistedState {
        crate::state::load(dir)
    }

    // --- toggle (T023), one test per row of the toggle table -------------------------------

    #[test]
    fn toggle_with_no_workspace_focused_is_a_silent_no_op() {
        let dir = temp_state_dir("toggle-no-workspace");
        let snapshot = empty_snapshot(); // focused_workspace_id: None, no context fallback
        let herdr = fake(snapshot, Ok(()));
        assert!(toggle(&herdr, &dir, None).is_ok());
        assert!(herdr.focus_calls.borrow().is_empty());
    }

    #[test]
    fn toggle_with_no_usable_focused_tab_does_not_call_observe() {
        // A workspace is focused, but focused_tab_id names no tab herdr reports for it -
        // the incomplete-snapshot row. observe must not run, so a workspace with prior history
        // is left exactly as repair leaves it, not overwritten by an unusable A.
        let dir = temp_state_dir("toggle-unusable-a");
        seed_state(&dir, {
            let mut s = PersistedState::default();
            s.workspaces
                .insert("wA".to_string(), history("wA:t1", Some("wA:t2")));
            s
        });
        let snapshot = Snapshot {
            focused_workspace_id: Some("wA".to_string()),
            focused_tab_id: Some("wA:t99-gone".to_string()),
            workspaces: vec![workspace("wA", "wA:t1")],
            tabs: vec![tab("wA:t1", "wA"), tab("wA:t2", "wA")],
        };
        let herdr = fake(snapshot, Ok(()));
        assert!(toggle(&herdr, &dir, None).is_ok());
        assert!(herdr.focus_calls.borrow().is_empty());
        assert_eq!(
            load_state(&dir).workspaces["wA"],
            history("wA:t1", Some("wA:t2"))
        );
    }

    #[test]
    fn toggle_is_a_no_op_when_last_tab_id_is_none_after_observe() {
        // A fresh workspace: observe seeds { current: A, last: None }, and there is nothing to
        // toggle to yet.
        let dir = temp_state_dir("toggle-no-history-yet");
        let snapshot = Snapshot {
            focused_workspace_id: Some("wA".to_string()),
            focused_tab_id: Some("wA:t1".to_string()),
            workspaces: vec![workspace("wA", "wA:t1")],
            tabs: vec![tab("wA:t1", "wA")],
        };
        let herdr = fake(snapshot, Ok(()));
        assert!(toggle(&herdr, &dir, None).is_ok());
        assert!(herdr.focus_calls.borrow().is_empty());
        assert_eq!(load_state(&dir).workspaces["wA"], history("wA:t1", None));
    }

    #[test]
    fn toggle_alternates_between_two_tabs() {
        let dir = temp_state_dir("toggle-alternate");
        seed_state(&dir, {
            let mut s = PersistedState::default();
            s.workspaces
                .insert("wA".to_string(), history("wA:t8", Some("wA:t1")));
            s
        });
        let snapshot = Snapshot {
            focused_workspace_id: Some("wA".to_string()),
            focused_tab_id: Some("wA:t8".to_string()),
            workspaces: vec![workspace("wA", "wA:t8")],
            tabs: vec![tab("wA:t1", "wA"), tab("wA:t8", "wA")],
        };
        let herdr = fake(snapshot, Ok(()));
        assert!(toggle(&herdr, &dir, None).is_ok());
        assert_eq!(herdr.focus_calls.borrow().as_slice(), ["wA:t1"]);
        assert_eq!(
            load_state(&dir).workspaces["wA"],
            history("wA:t1", Some("wA:t8"))
        );
    }

    #[test]
    fn toggle_clears_the_target_when_tab_focus_reports_it_gone() {
        let dir = temp_state_dir("toggle-target-gone");
        seed_state(&dir, {
            let mut s = PersistedState::default();
            s.workspaces
                .insert("wA".to_string(), history("wA:t8", Some("wA:t1")));
            s
        });
        let snapshot = Snapshot {
            focused_workspace_id: Some("wA".to_string()),
            focused_tab_id: Some("wA:t8".to_string()),
            workspaces: vec![workspace("wA", "wA:t8")],
            tabs: vec![tab("wA:t1", "wA"), tab("wA:t8", "wA")],
        };
        let herdr = fake(snapshot, Err(HerdrError::TabNotFound));
        assert!(toggle(&herdr, &dir, None).is_ok());
        assert_eq!(load_state(&dir).workspaces["wA"], history("wA:t8", None));
    }

    #[test]
    fn toggle_reports_a_non_tab_not_found_focus_failure() {
        let dir = temp_state_dir("toggle-focus-failure");
        seed_state(&dir, {
            let mut s = PersistedState::default();
            s.workspaces
                .insert("wA".to_string(), history("wA:t8", Some("wA:t1")));
            s
        });
        let snapshot = Snapshot {
            focused_workspace_id: Some("wA".to_string()),
            focused_tab_id: Some("wA:t8".to_string()),
            workspaces: vec![workspace("wA", "wA:t8")],
            tabs: vec![tab("wA:t1", "wA"), tab("wA:t8", "wA")],
        };
        let herdr = fake(
            snapshot,
            Err(HerdrError::Reported("herdr says no".to_string())),
        );
        let err = toggle(&herdr, &dir, None).unwrap_err();
        assert_eq!(err, "herdr says no");
        // Writing nothing on an `Err` is a deliberate contract (see the Command bodies header),
        // not an accident of where the early return happens to sit. Asserting only the message
        // would let a future change persist before surfacing the failure and still pass.
        assert_eq!(
            load_state(&dir).workspaces["wA"],
            history("wA:t8", Some("wA:t1")),
            "a surfaced focus failure persists nothing; repair is left for the next invocation"
        );
    }

    // --- tab-focused (T024) -----------------------------------------------------------------

    #[test]
    fn tab_focused_with_absent_event_json_changes_nothing() {
        let dir = temp_state_dir("tab-focused-absent");
        seed_state(&dir, {
            let mut s = PersistedState::default();
            s.workspaces
                .insert("wA".to_string(), history("wA:t1", None));
            s
        });
        let snapshot = Snapshot {
            focused_workspace_id: Some("wA".to_string()),
            focused_tab_id: Some("wA:t1".to_string()),
            workspaces: vec![workspace("wA", "wA:t1")],
            tabs: vec![tab("wA:t1", "wA")],
        };
        let herdr = fake(snapshot, Ok(()));
        assert!(tab_focused(&herdr, &dir, None).is_ok());
        assert_eq!(load_state(&dir).workspaces["wA"], history("wA:t1", None));
    }

    #[test]
    fn tab_focused_with_unparseable_event_json_changes_nothing() {
        let dir = temp_state_dir("tab-focused-unparseable");
        let snapshot = empty_snapshot();
        let herdr = fake(snapshot, Ok(()));
        assert!(tab_focused(&herdr, &dir, Some("not json")).is_ok());
        assert!(load_state(&dir).workspaces.is_empty());
    }

    #[test]
    fn tab_focused_with_missing_fields_changes_nothing() {
        let dir = temp_state_dir("tab-focused-missing-fields");
        let snapshot = empty_snapshot();
        let herdr = fake(snapshot, Ok(()));
        let payload = r#"{"event":"tab.focused","data":{"tab_id":"wA:t1"}}"#; // no workspace_id
        assert!(tab_focused(&herdr, &dir, Some(payload)).is_ok());
        assert!(load_state(&dir).workspaces.is_empty());
    }

    #[test]
    fn tab_focused_discards_a_stale_observation() {
        // The payload names a tab the snapshot does not currently report as focused in that
        // workspace - stale, and must be discarded (FR-011).
        let dir = temp_state_dir("tab-focused-stale");
        let snapshot = Snapshot {
            focused_workspace_id: Some("wA".to_string()),
            focused_tab_id: Some("wA:t2".to_string()), // something else is focused now
            workspaces: vec![workspace("wA", "wA:t2")],
            tabs: vec![tab("wA:t1", "wA"), tab("wA:t2", "wA")],
        };
        let herdr = fake(snapshot, Ok(()));
        let payload = r#"{"event":"tab.focused","data":{"tab_id":"wA:t1","workspace_id":"wA"}}"#;
        assert!(tab_focused(&herdr, &dir, Some(payload)).is_ok());
        assert!(load_state(&dir).workspaces.is_empty());
    }

    #[test]
    fn tab_focused_observes_a_currently_focused_tab() {
        let dir = temp_state_dir("tab-focused-observes");
        let snapshot = Snapshot {
            focused_workspace_id: Some("wA".to_string()),
            focused_tab_id: Some("wA:t1".to_string()),
            workspaces: vec![workspace("wA", "wA:t1")],
            tabs: vec![tab("wA:t1", "wA")],
        };
        let herdr = fake(snapshot, Ok(()));
        let payload = r#"{"event":"tab.focused","data":{"tab_id":"wA:t1","workspace_id":"wA"}}"#;
        assert!(tab_focused(&herdr, &dir, Some(payload)).is_ok());
        assert_eq!(load_state(&dir).workspaces["wA"], history("wA:t1", None));
    }

    // Three regressions, each starting from a `tab.closed` missed while the plugin was not
    // running - the state still names the dead tab, and only the next invocation's repair
    // pass ever sees it.

    #[test]
    fn tab_focused_on_a_different_tab_does_not_promote_a_dead_current() {
        let dir = temp_state_dir("tab-focused-regression-different-tab");
        seed_state(&dir, {
            let mut s = PersistedState::default();
            // wA:t1 (current) has since closed; herdr never delivered the tab.closed event.
            s.workspaces
                .insert("wA".to_string(), history("wA:t1-closed", None));
            s
        });
        let snapshot = Snapshot {
            focused_workspace_id: Some("wA".to_string()),
            focused_tab_id: Some("wA:t2".to_string()),
            workspaces: vec![workspace("wA", "wA:t2")],
            tabs: vec![tab("wA:t2", "wA")],
        };
        let herdr = fake(snapshot, Ok(()));
        let payload = r#"{"event":"tab.focused","data":{"tab_id":"wA:t2","workspace_id":"wA"}}"#;
        assert!(tab_focused(&herdr, &dir, Some(payload)).is_ok());
        // Repair replaced the dead current with wA:t2 (the workspace's now-active tab) before
        // observe ran; since observe's T (wA:t2) already equals that repaired current, the
        // middle case applies and no dead ID is ever promoted to last_tab_id.
        assert_eq!(load_state(&dir).workspaces["wA"], history("wA:t2", None));
    }

    #[test]
    fn tab_focused_on_the_unchanged_current_still_drops_a_dead_target() {
        let dir = temp_state_dir("tab-focused-regression-unchanged-current");
        seed_state(&dir, {
            let mut s = PersistedState::default();
            s.workspaces
                .insert("wA".to_string(), history("wA:t1", Some("wA:t9-closed")));
            s
        });
        let snapshot = Snapshot {
            focused_workspace_id: Some("wA".to_string()),
            focused_tab_id: Some("wA:t1".to_string()),
            workspaces: vec![workspace("wA", "wA:t1")],
            tabs: vec![tab("wA:t1", "wA")],
        };
        let herdr = fake(snapshot, Ok(()));
        let payload = r#"{"event":"tab.focused","data":{"tab_id":"wA:t1","workspace_id":"wA"}}"#;
        assert!(tab_focused(&herdr, &dir, Some(payload)).is_ok());
        assert_eq!(load_state(&dir).workspaces["wA"], history("wA:t1", None));
    }

    #[test]
    fn tab_focused_promotes_a_still_live_previous_current_normally() {
        let dir = temp_state_dir("tab-focused-regression-ordinary-promotion");
        seed_state(&dir, {
            let mut s = PersistedState::default();
            s.workspaces
                .insert("wA".to_string(), history("wA:t1", None));
            s
        });
        let snapshot = Snapshot {
            focused_workspace_id: Some("wA".to_string()),
            focused_tab_id: Some("wA:t2".to_string()),
            workspaces: vec![workspace("wA", "wA:t2")],
            tabs: vec![tab("wA:t1", "wA"), tab("wA:t2", "wA")],
        };
        let herdr = fake(snapshot, Ok(()));
        let payload = r#"{"event":"tab.focused","data":{"tab_id":"wA:t2","workspace_id":"wA"}}"#;
        assert!(tab_focused(&herdr, &dir, Some(payload)).is_ok());
        assert_eq!(
            load_state(&dir).workspaces["wA"],
            history("wA:t2", Some("wA:t1"))
        );
    }

    // --- tab-closed (T025): no transition of its own ----------------------------------------

    #[test]
    fn tab_closed_leaves_the_previous_tab_as_a_usable_target() {
        let dir = temp_state_dir("tab-closed-usable-target");
        seed_state(&dir, {
            let mut s = PersistedState::default();
            s.workspaces
                .insert("wA".to_string(), history("wA:t8-closed", Some("wA:t1")));
            s
        });
        let snapshot = Snapshot {
            focused_workspace_id: Some("wA".to_string()),
            focused_tab_id: Some("wA:t1".to_string()),
            workspaces: vec![workspace("wA", "wA:t1")],
            tabs: vec![tab("wA:t1", "wA")],
        };
        let herdr = fake(snapshot, Ok(()));
        let payload =
            r#"{"event":"tab.closed","data":{"tab_id":"wA:t8-closed","workspace_id":"wA"}}"#;
        assert!(tab_closed(&herdr, &dir, Some(payload)).is_ok());
        assert_eq!(load_state(&dir).workspaces["wA"], history("wA:t1", None));
    }

    #[test]
    fn tab_closed_does_not_let_a_stale_last_survive_beside_a_replaced_current() {
        let dir = temp_state_dir("tab-closed-stale-last");
        seed_state(&dir, {
            let mut s = PersistedState::default();
            s.workspaces.insert(
                "wA".to_string(),
                history("wA:t8-closed", Some("wA:t9-also-closed")),
            );
            s
        });
        let snapshot = Snapshot {
            focused_workspace_id: Some("wA".to_string()),
            focused_tab_id: Some("wA:t1".to_string()),
            workspaces: vec![workspace("wA", "wA:t1")],
            tabs: vec![tab("wA:t1", "wA")],
        };
        let herdr = fake(snapshot, Ok(()));
        let payload =
            r#"{"event":"tab.closed","data":{"tab_id":"wA:t8-closed","workspace_id":"wA"}}"#;
        assert!(tab_closed(&herdr, &dir, Some(payload)).is_ok());
        assert_eq!(load_state(&dir).workspaces["wA"], history("wA:t1", None));
    }

    // --- workspace isolation (US2: T033, T034) ----------------------------------------------

    #[test]
    fn toggle_in_one_workspace_leaves_another_workspaces_entry_byte_identical() {
        let dir = temp_state_dir("isolation-untouched-entry");
        seed_state(&dir, {
            let mut s = PersistedState::default();
            s.workspaces
                .insert("wA".to_string(), history("wA:t8", Some("wA:t1")));
            s.workspaces
                .insert("wB".to_string(), history("wB:t2", Some("wB:t1")));
            s
        });
        let snapshot = Snapshot {
            focused_workspace_id: Some("wA".to_string()),
            focused_tab_id: Some("wA:t8".to_string()),
            workspaces: vec![workspace("wA", "wA:t8"), workspace("wB", "wB:t2")],
            tabs: vec![
                tab("wA:t1", "wA"),
                tab("wA:t8", "wA"),
                tab("wB:t1", "wB"),
                tab("wB:t2", "wB"),
            ],
        };
        let herdr = fake(snapshot, Ok(()));
        assert!(toggle(&herdr, &dir, None).is_ok());
        assert_eq!(herdr.focus_calls.borrow().as_slice(), ["wA:t1"]);
        assert_eq!(
            load_state(&dir).workspaces["wB"],
            history("wB:t2", Some("wB:t1")),
            "a toggle in wA must never touch wB's entry"
        );
    }

    #[test]
    fn toggle_only_ever_focuses_a_tab_the_snapshot_reports_in_the_focused_workspace() {
        // Same setup as above, phrased as the FR-005/FR-006 assertion directly: the one
        // `focus_tab` call named a tab of the focused workspace, and nothing about the call
        // itself could have moved focus to a different workspace (there is no such operation
        // in the `Herdr` seam at all - `focus_tab` takes only a tab id).
        let dir = temp_state_dir("isolation-focus-target");
        seed_state(&dir, {
            let mut s = PersistedState::default();
            s.workspaces
                .insert("wA".to_string(), history("wA:t8", Some("wA:t1")));
            s
        });
        let snapshot = Snapshot {
            focused_workspace_id: Some("wA".to_string()),
            focused_tab_id: Some("wA:t8".to_string()),
            workspaces: vec![workspace("wA", "wA:t8")],
            tabs: vec![tab("wA:t1", "wA"), tab("wA:t8", "wA")],
        };
        let herdr = fake(snapshot, Ok(()));
        assert!(toggle(&herdr, &dir, None).is_ok());
        let calls = herdr.focus_calls.borrow();
        assert_eq!(calls.as_slice(), ["wA:t1"]);
    }

    #[test]
    fn toggle_in_a_workspace_with_no_entry_is_a_no_op_regardless_of_other_history() {
        let dir = temp_state_dir("isolation-no-entry-for-wb");
        seed_state(&dir, {
            let mut s = PersistedState::default();
            s.workspaces
                .insert("wA".to_string(), history("wA:t8", Some("wA:t1")));
            s
        });
        let snapshot = Snapshot {
            focused_workspace_id: Some("wB".to_string()),
            focused_tab_id: Some("wB:t1".to_string()),
            workspaces: vec![workspace("wA", "wA:t8"), workspace("wB", "wB:t1")],
            tabs: vec![tab("wA:t1", "wA"), tab("wA:t8", "wA"), tab("wB:t1", "wB")],
        };
        let herdr = fake(snapshot, Ok(()));
        assert!(toggle(&herdr, &dir, None).is_ok());
        assert!(herdr.focus_calls.borrow().is_empty());
        assert_eq!(
            load_state(&dir).workspaces["wA"],
            history("wA:t8", Some("wA:t1")),
            "wA's history is untouched by a toggle that resolved to wB"
        );
    }

    /// US2 scenario 4, and the exact bug spec.md's prior art has: activating a workspace that
    /// has never had its tabs switched re-reports its already-active tab as "focused", which
    /// must seed no history and must not disturb any other workspace's target.
    #[test]
    fn activating_a_workspace_seeds_no_history_and_leaves_other_workspaces_alone() {
        let dir = temp_state_dir("isolation-activation-seeds-nothing");
        seed_state(&dir, {
            let mut s = PersistedState::default();
            s.workspaces
                .insert("wA".to_string(), history("wA:t8", Some("wA:t1")));
            s
        });
        // wB has never had its tabs switched: activating it re-reports wB:t1, its
        // already-active tab, as the payload of a tab.focused event.
        let snapshot = Snapshot {
            focused_workspace_id: Some("wB".to_string()),
            focused_tab_id: Some("wB:t1".to_string()),
            workspaces: vec![workspace("wA", "wA:t8"), workspace("wB", "wB:t1")],
            tabs: vec![tab("wA:t1", "wA"), tab("wA:t8", "wA"), tab("wB:t1", "wB")],
        };
        let herdr = fake(snapshot, Ok(()));
        let payload = r#"{"event":"tab.focused","data":{"tab_id":"wB:t1","workspace_id":"wB"}}"#;
        assert!(tab_focused(&herdr, &dir, Some(payload)).is_ok());

        let state = load_state(&dir);
        assert_eq!(
            state.workspaces["wB"],
            history("wB:t1", None),
            "activation seeds current with no target, never a transition"
        );
        assert_eq!(
            state.workspaces["wA"],
            history("wA:t8", Some("wA:t1")),
            "wA's history must be untouched by activity in wB"
        );
    }

    // --- workspace-closed (US2: T035): no transition of its own -----------------------------

    #[test]
    fn workspace_closed_applies_no_transition_and_prunes_via_repair() {
        let dir = temp_state_dir("workspace-closed-prunes");
        seed_state(&dir, {
            let mut s = PersistedState::default();
            s.workspaces
                .insert("wA".to_string(), history("wA:t8", Some("wA:t1")));
            s.workspaces
                .insert("wB".to_string(), history("wB:t1", None));
            s
        });
        // wB has closed: the snapshot no longer reports it as a workspace at all.
        let snapshot = Snapshot {
            focused_workspace_id: Some("wA".to_string()),
            focused_tab_id: Some("wA:t8".to_string()),
            workspaces: vec![workspace("wA", "wA:t8")],
            tabs: vec![tab("wA:t1", "wA"), tab("wA:t8", "wA")],
        };
        let herdr = fake(snapshot, Ok(()));
        let payload = r#"{"event":"workspace.closed","data":{"workspace_id":"wB"}}"#;
        assert!(workspace_closed(&herdr, &dir, Some(payload)).is_ok());

        let state = load_state(&dir);
        assert!(
            !state.workspaces.contains_key("wB"),
            "wB's entry is dropped"
        );
        assert_eq!(
            state.workspaces["wA"],
            history("wA:t8", Some("wA:t1")),
            "workspace-closed applies no transition of its own"
        );
    }

    #[test]
    fn tab_closed_never_branches_on_the_payloads_tab_id() {
        // A payload naming a tab utterly unrelated to any stored entry must behave identically
        // to no payload at all: tab-closed applies no transition of its own, so nothing in it
        // is used for anything but confirming the payload is well-formed.
        //
        // Both runs need a seeded entry and a snapshot with live tabs in them. Run against an
        // empty state directory the two outcomes are trivially equal — there is nothing a
        // branch on `tab_id` could have changed — so the test would pass even if `tab_closed`
        // did read the payload, which is the one thing it exists to rule out.
        let snapshot = Snapshot {
            focused_workspace_id: Some("wA".to_string()),
            focused_tab_id: Some("wA:t8".to_string()),
            workspaces: vec![workspace("wA", "wA:t8")],
            tabs: vec![tab("wA:t1", "wA"), tab("wA:t8", "wA")],
        };
        let seeded = |label: &str| {
            let dir = temp_state_dir(label);
            let mut state = PersistedState::default();
            state
                .workspaces
                .insert("wA".to_string(), history("wA:t8", Some("wA:t1")));
            seed_state(&dir, state);
            dir
        };
        let payload =
            r#"{"event":"tab.closed","data":{"tab_id":"unrelated","workspace_id":"unrelated"}}"#;

        let with_payload = seeded("tab-closed-ignores-payload-with");
        let herdr = fake(snapshot.clone(), Ok(()));
        assert!(tab_closed(&herdr, &with_payload, Some(payload)).is_ok());

        let without_payload = seeded("tab-closed-ignores-payload-without");
        let herdr = fake(snapshot, Ok(()));
        assert!(tab_closed(&herdr, &without_payload, None).is_ok());

        assert_eq!(
            load_state(&with_payload),
            load_state(&without_payload),
            "a payload naming an unrelated tab must leave exactly what no payload leaves"
        );
    }

    #[test]
    fn required_env_rejects_absent_variable() {
        let name = "HERDR_LAST_TAB_TEST_ABSENT_VAR";
        std::env::remove_var(name);
        let err = required_env(name).unwrap_err();
        assert!(err.contains(name));
    }

    #[test]
    fn required_env_rejects_empty_variable() {
        let name = "HERDR_LAST_TAB_TEST_EMPTY_VAR";
        std::env::set_var(name, "");
        let err = required_env(name).unwrap_err();
        std::env::remove_var(name);
        assert!(err.contains(name));
    }

    #[test]
    fn required_env_accepts_nonempty_variable() {
        let name = "HERDR_LAST_TAB_TEST_PRESENT_VAR";
        std::env::set_var(name, "/usr/bin/herdr");
        let value = required_env(name).unwrap();
        std::env::remove_var(name);
        assert_eq!(value, "/usr/bin/herdr");
    }

    #[test]
    fn event_field_reads_nested_data_field() {
        let json = r#"{"event":"tab.focused","data":{"tab_id":"wA:t8","workspace_id":"wA"}}"#;
        assert_eq!(event_field(json, "tab_id").as_deref(), Some("wA:t8"));
        assert_eq!(event_field(json, "workspace_id").as_deref(), Some("wA"));
    }

    #[test]
    fn event_field_falls_back_to_top_level_field() {
        let json = r#"{"tab_id":"wA:t8","workspace_id":"wA"}"#;
        assert_eq!(event_field(json, "tab_id").as_deref(), Some("wA:t8"));
    }

    #[test]
    fn event_field_is_none_for_malformed_or_missing_json() {
        assert_eq!(event_field("not json", "tab_id"), None);
        assert_eq!(event_field(r#"{"data":{}}"#, "tab_id"), None);
    }

    /// A nested value that is present but unusable must not shadow a usable top-level one. The
    /// fallback exists precisely for payload shapes this plugin did not anticipate, so it has to
    /// key on whether the nested candidate is a string rather than on whether it is there.
    #[test]
    fn event_field_falls_back_when_the_nested_value_is_not_a_string() {
        for nested in ["null", "42", "{}", "[]", "true"] {
            let json = format!(r#"{{"data":{{"tab_id":{nested}}},"tab_id":"wA:t8"}}"#);
            assert_eq!(
                event_field(&json, "tab_id").as_deref(),
                Some("wA:t8"),
                "a nested {nested} should not shadow the top-level string"
            );
        }
    }

    #[test]
    fn context_workspace_id_reads_the_field() {
        let json = r#"{"workspace_id":"wA","tab_id":"wA:t8"}"#;
        assert_eq!(context_workspace_id(json).as_deref(), Some("wA"));
    }

    #[test]
    fn context_workspace_id_is_none_for_malformed_or_absent_field() {
        assert_eq!(context_workspace_id("not json"), None);
        assert_eq!(context_workspace_id(r#"{"tab_id":"wA:t8"}"#), None);
    }

    #[test]
    fn every_command_body_succeeds_on_an_empty_snapshot() {
        let dir = temp_state_dir("all-bodies-empty-snapshot");
        let fake = fake(empty_snapshot(), Ok(()));
        assert!(toggle(&fake, &dir, None).is_ok());
        assert!(tab_focused(&fake, &dir, None).is_ok());
        assert!(tab_closed(&fake, &dir, None).is_ok());
        assert!(workspace_closed(&fake, &dir, None).is_ok());
    }

    #[test]
    fn every_command_body_surfaces_an_unreachable_herdr() {
        let dir = temp_state_dir("all-bodies-unreachable");
        let herdr = FakeHerdr {
            snapshot: Err(HerdrError::Unreachable(
                "herdr could not be reached".to_string(),
            )),
            focus_result: Ok(()),
            focus_calls: RefCell::new(Vec::new()),
        };
        assert_eq!(
            toggle(&herdr, &dir, None).unwrap_err(),
            "herdr could not be reached"
        );
        assert_eq!(
            tab_focused(&herdr, &dir, None).unwrap_err(),
            "herdr could not be reached"
        );
        assert_eq!(
            tab_closed(&herdr, &dir, None).unwrap_err(),
            "herdr could not be reached"
        );
        assert_eq!(
            workspace_closed(&herdr, &dir, None).unwrap_err(),
            "herdr could not be reached"
        );
    }
}
