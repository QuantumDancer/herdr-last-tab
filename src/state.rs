//! The persisted seam (Principle II): one JSON file under an exclusive lock, holding the
//! per-workspace focus history the toggle reads and writes. See data-model.md for the shape
//! and the concurrency argument this module implements.

use crate::herdr::Snapshot;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The whole of what the plugin remembers, keyed by herdr's workspace identifier.
///
/// `BTreeMap` rather than `HashMap`: a `HashMap`'s iteration order is randomised per process, so
/// two runs that make the identical change would serialize to different byte sequences and a
/// concurrency test could only compare parsed values, never raw files. `BTreeMap` keys sort, so
/// the same logical state always serializes identically — which is what lets the SC-009 test
/// diff `state.json` directly instead of parsing and re-comparing it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedState {
    pub workspaces: BTreeMap<String, WorkspaceHistory>,
}

/// One workspace's two-deep focus history. `last_tab_id` is the toggle target; `None` means the
/// workspace's current tab is known but it has never been switched (invariant 4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceHistory {
    pub current_tab_id: String,
    pub last_tab_id: Option<String>,
}

impl PersistedState {
    /// Repairs the whole map against `snapshot`, then leaves it for the caller to apply at most
    /// one transition. This is one of the two halves of "repair, then transition"
    /// (data-model.md#repair-then-transition): repair drops what is dead, and only afterwards
    /// does a transition rule decide whether anything happened. Folding a repair concern into a
    /// transition rule is the mistake research.md's "one repair pass" records three separate
    /// review rounds finding - each fix patched the one branch it was written for while the
    /// identical hole sat in the next rule untouched. Keeping repair as its own pass, run
    /// unconditionally before any transition, is what makes FR-002 ("an observation naming the
    /// already-current tab changes nothing") and FR-009 ("a no-op still repairs") consistent
    /// instead of contradictory: they describe different phases.
    ///
    /// Runs over **every** entry, not only the one a subcommand is about to act on - a stale ID
    /// in another workspace is equally a violation of the invariants below, and the map is
    /// already in hand. Scoping this to "the current entry" is the narrower version of the same
    /// mistake and is exactly what the review rounds above kept re-finding.
    pub fn repair(&mut self, snapshot: &Snapshot) {
        // Step 1 (T038): an entry whose workspace has closed altogether has no active_tab_id
        // left to fall back to, so nothing steps 2-4 do could save it - it is dropped outright,
        // ahead of everything else, rather than let a later step trip over a workspace that no
        // longer exists. This is invariant 1, and it only holds "whenever the plugin releases
        // the lock" (data-model.md#invariants): a workspace closed while the plugin was not
        // running leaves a stale entry that nothing removes until the plugin next runs, which
        // is why this pass runs unconditionally on every subcommand rather than only on
        // `workspace-closed`.
        self.workspaces
            .retain(|workspace_id, _| snapshot_has_workspace(snapshot, workspace_id));

        // Steps 2-4: for every surviving entry, replace or discard a dead `current_tab_id`,
        // clear a dead `last_tab_id`, and clear a `last_tab_id` that has collapsed onto
        // `current_tab_id` (invariant 2). A `retain` that also mutates the surviving values is
        // exactly the shape this pass needs: it is one linear walk that both fixes and prunes,
        // rather than two passes that could disagree about which entries survived.
        self.workspaces.retain(|workspace_id, history| {
            let live_tab_count = snapshot
                .tabs
                .iter()
                .filter(|tab| tab.workspace_id == *workspace_id)
                .count();
            if live_tab_count == 0 {
                // Step 2's other half: a workspace with nothing live left has no active tab to
                // fall back to, so the entry cannot be repaired - only dropped.
                return false;
            }

            let current_is_live = snapshot.tabs.iter().any(|tab| {
                tab.workspace_id == *workspace_id && tab.tab_id == history.current_tab_id
            });
            if !current_is_live {
                // Step 1 already proved this workspace is still in the snapshot, so its
                // `active_tab_id` is available to fall back to.
                let active = snapshot
                    .workspaces
                    .iter()
                    .find(|w| w.workspace_id == *workspace_id)
                    .map(|w| w.active_tab_id.clone())
                    .expect("workspace survived step 1's retain, so it is present");

                // The fallback is only usable if this same snapshot also reports it as a live
                // tab of this workspace. Nothing in herdr's schema forces `active_tab_id` to
                // appear in `tabs`, and a snapshot that disagrees with itself must not be
                // written back as a repaired current - `observe` documents `current_tab_id` as
                // guaranteed live once repair has run, and would otherwise promote a dead ID
                // into `last_tab_id`, leaving the toggle to chase a tab that does not exist.
                // Dropping the entry is the same answer step 2's other half gives when there is
                // no live tab to fall back to at all; this is that case, arrived at differently.
                let active_is_live = snapshot
                    .tabs
                    .iter()
                    .any(|tab| tab.workspace_id == *workspace_id && tab.tab_id == active);
                if !active_is_live {
                    return false;
                }
                history.current_tab_id = active;
            }

            if let Some(last_tab_id) = &history.last_tab_id {
                let last_is_live = snapshot
                    .tabs
                    .iter()
                    .any(|tab| tab.workspace_id == *workspace_id && &tab.tab_id == last_tab_id);
                if !last_is_live {
                    history.last_tab_id = None;
                }
            }

            if history.last_tab_id.as_deref() == Some(history.current_tab_id.as_str()) {
                history.last_tab_id = None;
            }

            true
        });
    }

    /// `observe(W, T, snapshot)` - T is now the current tab of W. The at-most-one transition
    /// applied after repair (data-model.md#transition--at-most-one-after-repair). Repair has
    /// already run by the time this is called, so `current_tab_id` is guaranteed live; this
    /// function never has to re-check it.
    pub fn observe(&mut self, workspace_id: &str, tab_id: &str, snapshot: &Snapshot) {
        // T not among the snapshot's tabs for W: the observation is stale (a hook that arrived
        // late, or a payload naming a tab that has since moved elsewhere) and is discarded
        // entirely rather than acted on (FR-011).
        let is_live = snapshot
            .tabs
            .iter()
            .any(|tab| tab.workspace_id == workspace_id && tab.tab_id == tab_id);
        if !is_live {
            return;
        }

        match self.workspaces.get(workspace_id) {
            None => {
                self.workspaces.insert(
                    workspace_id.to_string(),
                    WorkspaceHistory {
                        current_tab_id: tab_id.to_string(),
                        last_tab_id: None,
                    },
                );
            }
            // The middle case does double duty (research.md#the-toggle-writes-its-own-swap):
            // it is why activating a workspace - which re-reports the tab already active there
            // - seeds no history for a workspace that had none, and overwrites nothing for one
            // that did. And it is why a `tab.focused` event that lands just after a toggle is
            // harmless: the toggle has already recorded the swap, so this event names the tab
            // that is already current and this arm changes nothing, whether or not a target is
            // currently held. "Almost right" here - e.g. refreshing `last_tab_id` anyway - is
            // exactly what breaks alternation, so this arm is a deliberate no-op, not an
            // oversight.
            Some(history) if history.current_tab_id == tab_id => {}
            Some(history) => {
                let previous_current = history.current_tab_id.clone();
                self.workspaces.insert(
                    workspace_id.to_string(),
                    WorkspaceHistory {
                        current_tab_id: tab_id.to_string(),
                        last_tab_id: Some(previous_current),
                    },
                );
            }
        }
    }
}

fn snapshot_has_workspace(snapshot: &Snapshot, workspace_id: &str) -> bool {
    snapshot
        .workspaces
        .iter()
        .any(|w| w.workspace_id == workspace_id)
}

pub(crate) const STATE_FILE: &str = "state.json";
const LOCK_FILE: &str = "state.lock";

/// Loads `state.json` from `dir`, treating anything that isn't a clean read of the expected
/// shape as empty state. A missing file, an empty file, a file truncated mid-object, and JSON
/// that doesn't match `PersistedState` are all the same case here: unreadable content is empty
/// content (FR-014, invariant 5). There is no error path — a corrupt file must never stop the
/// plugin from running, and the next successful `persist` repairs it.
pub fn load(dir: &Path) -> PersistedState {
    fs::read(dir.join(STATE_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// An exclusive lock on `state.lock`, held for the caller's entire read-modify-write.
///
/// The lock is acquired well before any state is read and released only when this guard is
/// dropped — deliberately spanning the `tab focus` call in between, not just the file I/O. A
/// narrower lock (around the read, then again around the write) would let two overlapping
/// toggles both read `{ current: A, last: B }`, both resolve `B` as the target, both call
/// `tab focus B`, and both write `{ current: B, last: A }` — a well-formed file that is wrong,
/// because the user pressed the key twice and never came back to `A`. Serializing the herdr
/// round-trip itself is the cost that buys alternation its correctness under concurrency
/// (data-model.md#concurrency).
pub struct StateLock {
    file: File,
    dir: PathBuf,
}

impl StateLock {
    /// Creates the state directory and lock file if absent, then blocks until the exclusive
    /// lock is acquired.
    pub fn acquire(dir: &Path) -> std::io::Result<Self> {
        fs::create_dir_all(dir)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.join(LOCK_FILE))?;
        file.lock_exclusive()?;
        Ok(Self {
            file,
            dir: dir.to_path_buf(),
        })
    }
}

impl Drop for StateLock {
    fn drop(&mut self) {
        // Best-effort: the OS also releases the lock when the file descriptor closes at
        // process exit, so a failure here (e.g. the fd already gone) has nothing left to do.
        let _ = FileExt::unlock(&self.file);
    }
}

/// Writes `state` durably and atomically, then keeps the lock held until `guard` is dropped by
/// the caller — this function does not release it, so the caller controls exactly how much of
/// the read-modify-write the lock spans.
///
/// Writing to a uniquely named temporary file in the same directory, `sync_all`-ing it, and then
/// `rename`-ing it over `state.json` is what makes the replacement atomic: a same-directory
/// `rename` is a single filesystem operation, so a concurrent reader (`load`, above) sees either
/// the old complete file or the new complete file and never a partial write.
pub fn persist(guard: &StateLock, state: &PersistedState) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(state)?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp_path = guard
        .dir
        .join(format!("state.{}.{nanos}.tmp", std::process::id()));

    let mut tmp = File::create(&tmp_path)?;
    tmp.write_all(&json)?;
    tmp.sync_all()?;
    drop(tmp);
    fs::rename(&tmp_path, guard.dir.join(STATE_FILE))?;

    // The rename is atomic for a concurrent *reader* the moment it returns, but it is a change to
    // the directory rather than to the file, so `tmp.sync_all()` above does not make it durable:
    // after power loss the entry can be missing while the file's contents are safely on disk.
    // Syncing the directory is what finishes the job the file sync starts — without it that first
    // sync buys nothing, since the data it flushed may have no name pointing at it.
    //
    // Both declared targets are POSIX, where opening a directory read-only and syncing it is the
    // supported way to do this. A crash here is not catastrophic — FR-014 makes a missing or
    // partial `state.json` a clean no-op that the next focus re-seeds — but the write protocol
    // claims durability, and a claim that holds only for the file half is worse than no claim.
    File::open(&guard.dir)?.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::herdr::{Snapshot, TabEntry, WorkspaceEntry};
    use std::collections::BTreeMap;

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

    /// A snapshot with nothing focused. Only the two focus fields are empty — the workspaces and
    /// tabs are the caller's — which is what repair's own tests need, since repair never reads
    /// the focus fields. Named for what is actually absent, to keep it distinct from `lib.rs`'s
    /// same-purpose helper of a different shape, which takes no arguments and is genuinely empty.
    fn unfocused_snapshot(workspaces: Vec<WorkspaceEntry>, tabs: Vec<TabEntry>) -> Snapshot {
        Snapshot {
            focused_workspace_id: None,
            focused_tab_id: None,
            workspaces,
            tabs,
        }
    }

    // --- repair (T021: tab-level steps 2-4; T038: workspace-level step 1) ------------------

    #[test]
    fn repair_replaces_a_dead_current_tab_with_the_workspace_active_tab() {
        let snapshot = unfocused_snapshot(vec![workspace("wA", "wA:t2")], vec![tab("wA:t2", "wA")]);
        let mut state = PersistedState::default();
        state
            .workspaces
            .insert("wA".to_string(), history("wA:t9-closed", None));
        state.repair(&snapshot);
        assert_eq!(state.workspaces["wA"].current_tab_id, "wA:t2");
    }

    #[test]
    fn repair_drops_an_entry_whose_workspace_names_an_active_tab_the_snapshot_does_not_list() {
        // A snapshot that disagrees with itself: wA is present and has a live tab, but its
        // `active_tab_id` names one absent from `tabs`. Falling back to it would write a dead
        // ID as the repaired current, which `observe` is documented to be able to trust.
        let snapshot = unfocused_snapshot(
            vec![workspace("wA", "wA:t9-absent")],
            vec![tab("wA:t2", "wA")],
        );
        let mut state = PersistedState::default();
        state
            .workspaces
            .insert("wA".to_string(), history("wA:t7-closed", None));
        state.repair(&snapshot);
        assert!(!state.workspaces.contains_key("wA"));
    }

    #[test]
    fn repair_drops_an_entry_whose_workspace_has_no_live_tabs_at_all() {
        // wA still exists as a workspace, but reports no tabs of its own - the "workspace with
        // nothing live left" case that step 2 has to distinguish from "current merely stale".
        let snapshot = unfocused_snapshot(vec![workspace("wA", "wA:t1")], vec![]);
        let mut state = PersistedState::default();
        state
            .workspaces
            .insert("wA".to_string(), history("wA:t1", None));
        state.repair(&snapshot);
        assert!(state.workspaces.is_empty());
    }

    #[test]
    fn repair_clears_a_dead_last_tab_id() {
        let snapshot = unfocused_snapshot(vec![workspace("wA", "wA:t2")], vec![tab("wA:t2", "wA")]);
        let mut state = PersistedState::default();
        state
            .workspaces
            .insert("wA".to_string(), history("wA:t2", Some("wA:t1-closed")));
        state.repair(&snapshot);
        assert_eq!(state.workspaces["wA"].last_tab_id, None);
    }

    #[test]
    fn repair_clears_a_last_tab_id_that_has_collapsed_onto_current() {
        let snapshot = unfocused_snapshot(
            vec![workspace("wA", "wA:t2")],
            vec![tab("wA:t2", "wA"), tab("wA:t1", "wA")],
        );
        let mut state = PersistedState::default();
        // A stored state that already violates invariant 2 (current == last) - repair must
        // not let it survive, whatever wrote it.
        state
            .workspaces
            .insert("wA".to_string(), history("wA:t2", Some("wA:t2")));
        state.repair(&snapshot);
        assert_eq!(state.workspaces["wA"].last_tab_id, None);
    }

    #[test]
    fn repair_drops_an_entry_whose_workspace_no_longer_exists() {
        let snapshot = unfocused_snapshot(vec![], vec![]);
        let mut state = PersistedState::default();
        state
            .workspaces
            .insert("wA".to_string(), history("wA:t1", None));
        state.repair(&snapshot);
        assert!(state.workspaces.is_empty());
    }

    /// The finding research.md records three review rounds for: repair must run over the
    /// *whole* map, never only the entry a subcommand is about to touch. Three workspaces,
    /// three unrelated violations (a dead workspace, a dead current, a dead last), one
    /// `repair()` call - all three must come out fixed.
    #[test]
    fn repair_fixes_stale_ids_across_every_workspace_not_just_one() {
        let snapshot = unfocused_snapshot(
            vec![workspace("wA", "wA:t2"), workspace("wB", "wB:t1")],
            vec![tab("wA:t2", "wA"), tab("wB:t1", "wB"), tab("wB:t2", "wB")],
        );
        let mut state = PersistedState::default();
        state
            .workspaces
            .insert("wA".to_string(), history("wA:t9-closed", None));
        state
            .workspaces
            .insert("wB".to_string(), history("wB:t1", Some("wB:t9-closed")));
        state
            .workspaces
            .insert("wC".to_string(), history("wC:t1", None)); // wC is gone from the snapshot

        state.repair(&snapshot);

        assert_eq!(state.workspaces.len(), 2, "wC must be dropped");
        assert_eq!(state.workspaces["wA"].current_tab_id, "wA:t2");
        assert_eq!(state.workspaces["wB"].last_tab_id, None);
        assert!(!state.workspaces.contains_key("wC"));
    }

    // --- observe (T022) ----------------------------------------------------------------------

    #[test]
    fn observe_discards_a_tab_the_snapshot_does_not_report_for_that_workspace() {
        let snapshot = unfocused_snapshot(vec![workspace("wA", "wA:t1")], vec![tab("wA:t1", "wA")]);
        let mut state = PersistedState::default();
        state.observe("wA", "wA:t99-not-reported", &snapshot);
        assert!(state.workspaces.is_empty());
    }

    #[test]
    fn observe_seeds_a_workspace_that_had_no_entry() {
        let snapshot = unfocused_snapshot(vec![workspace("wA", "wA:t1")], vec![tab("wA:t1", "wA")]);
        let mut state = PersistedState::default();
        state.observe("wA", "wA:t1", &snapshot);
        assert_eq!(state.workspaces["wA"], history("wA:t1", None));
    }

    #[test]
    fn observe_leaves_an_already_current_entry_untouched_when_it_holds_a_target() {
        let snapshot = unfocused_snapshot(
            vec![workspace("wA", "wA:t1")],
            vec![tab("wA:t1", "wA"), tab("wA:t2", "wA")],
        );
        let mut state = PersistedState::default();
        state
            .workspaces
            .insert("wA".to_string(), history("wA:t1", Some("wA:t2")));
        state.observe("wA", "wA:t1", &snapshot);
        assert_eq!(state.workspaces["wA"], history("wA:t1", Some("wA:t2")));
    }

    #[test]
    fn observe_leaves_an_already_current_entry_untouched_when_it_holds_no_target() {
        let snapshot = unfocused_snapshot(vec![workspace("wA", "wA:t1")], vec![tab("wA:t1", "wA")]);
        let mut state = PersistedState::default();
        state
            .workspaces
            .insert("wA".to_string(), history("wA:t1", None));
        state.observe("wA", "wA:t1", &snapshot);
        assert_eq!(state.workspaces["wA"], history("wA:t1", None));
    }

    #[test]
    fn observe_promotes_the_previous_current_to_last() {
        let snapshot = unfocused_snapshot(
            vec![workspace("wA", "wA:t2")],
            vec![tab("wA:t1", "wA"), tab("wA:t2", "wA")],
        );
        let mut state = PersistedState::default();
        state
            .workspaces
            .insert("wA".to_string(), history("wA:t1", None));
        state.observe("wA", "wA:t2", &snapshot);
        assert_eq!(state.workspaces["wA"], history("wA:t2", Some("wA:t1")));
    }

    /// A fresh directory under the system temp dir, unique per test so parallel `cargo test`
    /// runs never collide.
    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "herdr-last-tab-state-test-{label}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_state() -> PersistedState {
        let mut workspaces = BTreeMap::new();
        workspaces.insert(
            "wA".to_string(),
            WorkspaceHistory {
                current_tab_id: "wA:t8".to_string(),
                last_tab_id: Some("wA:t1".to_string()),
            },
        );
        PersistedState { workspaces }
    }

    #[test]
    fn missing_file_loads_as_empty_map() {
        let dir = temp_dir("missing");
        assert_eq!(load(&dir), PersistedState::default());
    }

    #[test]
    fn empty_file_loads_as_empty_map() {
        let dir = temp_dir("empty");
        fs::write(dir.join(STATE_FILE), b"").unwrap();
        assert_eq!(load(&dir), PersistedState::default());
    }

    #[test]
    fn non_json_file_loads_as_empty_map() {
        let dir = temp_dir("not-json");
        fs::write(dir.join(STATE_FILE), b"not json").unwrap();
        assert_eq!(load(&dir), PersistedState::default());
    }

    #[test]
    fn truncated_mid_object_loads_as_empty_map() {
        let dir = temp_dir("truncated");
        let full = serde_json::to_vec(&sample_state()).unwrap();
        fs::write(dir.join(STATE_FILE), &full[..full.len() / 2]).unwrap();
        assert_eq!(load(&dir), PersistedState::default());
    }

    #[test]
    fn corrupt_state_is_repaired_by_the_next_write() {
        let dir = temp_dir("repair");
        fs::write(dir.join(STATE_FILE), b"not json").unwrap();
        let guard = StateLock::acquire(&dir).unwrap();
        persist(&guard, &sample_state()).unwrap();
        drop(guard);
        assert_eq!(load(&dir), sample_state());
    }

    #[test]
    fn persist_produces_valid_json_with_stable_key_order() {
        let dir = temp_dir("stable-order");
        let mut workspaces = BTreeMap::new();
        workspaces.insert(
            "wZ".to_string(),
            WorkspaceHistory {
                current_tab_id: "wZ:t1".to_string(),
                last_tab_id: None,
            },
        );
        workspaces.insert(
            "wA".to_string(),
            WorkspaceHistory {
                current_tab_id: "wA:t1".to_string(),
                last_tab_id: None,
            },
        );
        let state = PersistedState { workspaces };

        let guard = StateLock::acquire(&dir).unwrap();
        persist(&guard, &state).unwrap();
        drop(guard);

        let raw = fs::read_to_string(dir.join(STATE_FILE)).unwrap();
        let wa = raw.find("\"wA\"").expect("wA present");
        let wz = raw.find("\"wZ\"").expect("wZ present");
        assert!(wa < wz, "keys must serialize in sorted order");

        // A second run producing the same logical state must produce byte-identical output.
        let dir2 = temp_dir("stable-order-2");
        let guard2 = StateLock::acquire(&dir2).unwrap();
        persist(&guard2, &state).unwrap();
        drop(guard2);
        let raw2 = fs::read_to_string(dir2.join(STATE_FILE)).unwrap();
        assert_eq!(raw, raw2);
    }

    #[test]
    fn persist_creates_no_file_outside_the_state_directory() {
        let dir = temp_dir("no-stray-files");
        let guard = StateLock::acquire(&dir).unwrap();
        persist(&guard, &sample_state()).unwrap();
        drop(guard);

        let entries: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        // Only the lock file and the final state file remain — the temporary file was renamed
        // away, not left behind.
        assert_eq!(entries.len(), 2, "unexpected entries: {entries:?}");
        assert!(entries.contains(&STATE_FILE.to_string()));
        assert!(entries.contains(&LOCK_FILE.to_string()));
    }
}
