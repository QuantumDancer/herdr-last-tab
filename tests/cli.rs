//! Integration tests: spawn the built `herdr-last-tab` binary itself, proving the process
//! boundary and the locking that a substituted `Herdr` trait object cannot — the second layer
//! research.md's "the herdr seam" calls for. `tests/fake-herdr.sh` stands in for herdr; see its
//! header comment for the fixtures it accepts.
//!
//! This Phase 2 slice (T020) covers only the failure exits reachable against the T016 stubs —
//! every one of them aborts before a command body would do anything. The remaining rows of the
//! exit-code contract (a real toggle's `0`-exit no-ops, and a `tab focus` failing with a
//! non-`tab_not_found` envelope) need command bodies that arrive in Phase 3 and land in T026.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_herdr-last-tab"))
}

fn fake_herdr() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fake-herdr.sh")
        .to_string_lossy()
        .into_owned()
}

/// A fresh, empty directory under the system temp dir, unique per call so parallel tests never
/// collide over the same `state.json`.
fn temp_state_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "herdr-last-tab-cli-test-{label}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A `Command` for the binary under test with every variable that can change its behaviour
/// explicitly cleared, so a test controls its whole environment rather than inheriting whatever
/// happens to be set in the process running `cargo test`.
///
/// The `FAKE_HERDR_*` fixtures belong in that list as much as the `HERDR_*` ones do. They are read
/// by `tests/fake-herdr.sh`, which runs as a child of the binary and therefore inherits them, so a
/// developer with `FAKE_HERDR_SNAPSHOT_ERROR` left over in their shell would watch
/// `every_subcommand_succeeds_against_a_working_fake_herdr` fail with exit 1 while CI stayed green
/// — the least debuggable shape a test failure can take.
fn command(subcommand: &str) -> Command {
    let mut cmd = Command::new(binary());
    cmd.arg(subcommand);
    cmd.env_remove("HERDR_BIN_PATH");
    cmd.env_remove("HERDR_PLUGIN_STATE_DIR");
    cmd.env_remove("HERDR_PLUGIN_EVENT_JSON");
    cmd.env_remove("HERDR_PLUGIN_CONTEXT_JSON");
    cmd.env_remove("FAKE_HERDR_SNAPSHOT_JSON");
    cmd.env_remove("FAKE_HERDR_SNAPSHOT_ERROR");
    cmd.env_remove("FAKE_HERDR_FOCUS_RESULT");
    cmd.env_remove("FAKE_HERDR_FOCUS_LOG");
    cmd.env_remove("FAKE_HERDR_FOCUS_DELAY");
    cmd.env_remove("FAKE_HERDR_FOCUS_STATE_FILE");
    cmd
}

/// A command pre-wired with a working `HERDR_BIN_PATH` (the fake herdr) and a fresh state
/// directory, so a test that only cares about one failure mode doesn't have to restate both.
fn valid_command(subcommand: &str, state_dir_label: &str) -> Command {
    let mut cmd = command(subcommand);
    cmd.env("HERDR_BIN_PATH", fake_herdr());
    cmd.env("HERDR_PLUGIN_STATE_DIR", temp_state_dir(state_dir_label));
    cmd
}

fn run(mut cmd: Command) -> Output {
    cmd.output().expect("failed to run herdr-last-tab")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn missing_bin_path_exits_1_naming_the_variable() {
    let mut cmd = command("toggle");
    cmd.env("HERDR_PLUGIN_STATE_DIR", temp_state_dir("missing-bin-path"));
    let output = run(cmd);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("HERDR_BIN_PATH"));
}

#[test]
fn empty_bin_path_exits_1_naming_the_variable() {
    let mut cmd = command("toggle");
    cmd.env("HERDR_BIN_PATH", "");
    cmd.env("HERDR_PLUGIN_STATE_DIR", temp_state_dir("empty-bin-path"));
    let output = run(cmd);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("HERDR_BIN_PATH"));
}

#[test]
fn missing_state_dir_exits_1_naming_the_variable() {
    let mut cmd = command("toggle");
    cmd.env("HERDR_BIN_PATH", fake_herdr());
    let output = run(cmd);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("HERDR_PLUGIN_STATE_DIR"));
}

#[test]
fn empty_state_dir_exits_1_naming_the_variable() {
    let mut cmd = command("toggle");
    cmd.env("HERDR_BIN_PATH", fake_herdr());
    cmd.env("HERDR_PLUGIN_STATE_DIR", "");
    let output = run(cmd);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("HERDR_PLUGIN_STATE_DIR"));
}

#[test]
fn herdr_that_cannot_be_spawned_exits_1() {
    let mut cmd = command("toggle");
    cmd.env("HERDR_BIN_PATH", "/nonexistent/herdr-last-tab-test-binary");
    cmd.env("HERDR_PLUGIN_STATE_DIR", temp_state_dir("spawn-failure"));
    let output = run(cmd);
    assert_eq!(output.status.code(), Some(1));
    assert!(!stderr(&output).is_empty());
}

#[test]
fn unparseable_herdr_response_exits_1() {
    // A snapshot object that is not valid JSON makes the whole `api snapshot` envelope
    // unparseable, which is exactly the "output that parses as neither result nor envelope"
    // row of the classification table.
    let mut cmd = valid_command("toggle", "unparseable-response");
    cmd.env("FAKE_HERDR_SNAPSHOT_JSON", "not valid json");
    let output = run(cmd);
    assert_eq!(output.status.code(), Some(1));
    assert!(!stderr(&output).is_empty());
}

#[test]
fn unknown_subcommand_exits_2_with_usage() {
    // Deliberately no environment set: a wiring mistake in the manifest must fail the same way
    // regardless of what herdr configured around the invocation.
    let cmd = command("nonsense");
    let output = run(cmd);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("usage"));
}

#[test]
fn missing_subcommand_exits_2_with_usage() {
    let output = Command::new(binary())
        .env_remove("HERDR_BIN_PATH")
        .env_remove("HERDR_PLUGIN_STATE_DIR")
        .output()
        .expect("failed to run herdr-last-tab");
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("usage"));
}

/// A `HERDR_PLUGIN_EVENT_JSON` value for a `tab.focused` event naming `tab_id` in `workspace_id`.
fn tab_focused_event(workspace_id: &str, tab_id: &str) -> String {
    format!(
        r#"{{"event":"tab.focused","data":{{"tab_id":"{tab_id}","workspace_id":"{workspace_id}"}}}}"#
    )
}

/// The snapshot JSON for one workspace holding exactly the given (live) tabs, with
/// `focused_workspace_id`/`focused_tab_id` naming the workspace and its currently focused tab.
fn snapshot_json(workspace_id: &str, active_tab_id: &str, tab_ids: &[&str]) -> String {
    let tabs: Vec<String> = tab_ids
        .iter()
        .map(|id| format!(r#"{{"tab_id":"{id}","workspace_id":"{workspace_id}"}}"#))
        .collect();
    format!(
        r#"{{"focused_workspace_id":"{workspace_id}","focused_tab_id":"{active_tab_id}","workspaces":[{{"workspace_id":"{workspace_id}","active_tab_id":"{active_tab_id}"}}],"tabs":[{}]}}"#,
        tabs.join(",")
    )
}

/// Runs `tab-focused` for `tab_id` in `workspace_id` against `state_dir` with a snapshot
/// reporting exactly `live_tab_ids` as live and `tab_id` as currently focused — the sequence a
/// real herdr session produces one focus-change at a time.
fn record_focus(state_dir: &PathBuf, workspace_id: &str, tab_id: &str, live_tab_ids: &[&str]) {
    let mut cmd = valid_command("tab-focused", "unused-because-state-dir-is-shared");
    cmd.env("HERDR_PLUGIN_STATE_DIR", state_dir);
    cmd.env(
        "FAKE_HERDR_SNAPSHOT_JSON",
        snapshot_json(workspace_id, tab_id, live_tab_ids),
    );
    cmd.env(
        "HERDR_PLUGIN_EVENT_JSON",
        tab_focused_event(workspace_id, tab_id),
    );
    let output = run(cmd);
    assert_eq!(
        output.status.code(),
        Some(0),
        "record_focus stderr: {}",
        stderr(&output)
    );
}

/// Runs `toggle` against `state_dir`, with the snapshot reporting `focused_tab_id` as currently
/// focused among `live_tab_ids` in `workspace_id`.
fn run_toggle(
    state_dir: &PathBuf,
    workspace_id: &str,
    focused_tab_id: &str,
    live_tab_ids: &[&str],
) -> Output {
    let mut cmd = valid_command("toggle", "unused-because-state-dir-is-shared");
    cmd.env("HERDR_PLUGIN_STATE_DIR", state_dir);
    cmd.env(
        "FAKE_HERDR_SNAPSHOT_JSON",
        snapshot_json(workspace_id, focused_tab_id, live_tab_ids),
    );
    run(cmd)
}

#[test]
fn toggle_alternates_between_two_tabs_and_no_ops_are_silent() {
    let dir = temp_state_dir("alternation");

    // Focus t1, then t3 — the harness's independent test for US1: "focus tab 1, focus tab 3,
    // toggle, confirm tab 1; toggle again, confirm tab 3".
    record_focus(&dir, "wA", "wA:t1", &["wA:t1", "wA:t3"]);
    record_focus(&dir, "wA", "wA:t3", &["wA:t1", "wA:t3"]);

    let first = run_toggle(&dir, "wA", "wA:t3", &["wA:t1", "wA:t3"]);
    assert_eq!(first.status.code(), Some(0));
    assert!(stderr(&first).is_empty());
    assert!(std::fs::read_to_string(dir.join("state.json"))
        .unwrap()
        .contains("\"wA:t1\""));

    // A second toggle, now reading the state the first one wrote, must land back on t3
    // (FR-003, SC-001).
    let second = run_toggle(&dir, "wA", "wA:t1", &["wA:t1", "wA:t3"]);
    assert_eq!(second.status.code(), Some(0));
    assert!(stderr(&second).is_empty());
    let raw = std::fs::read_to_string(dir.join("state.json")).unwrap();
    assert!(raw.contains("\"current_tab_id\": \"wA:t3\""));
    assert!(raw.contains("\"last_tab_id\": \"wA:t1\""));
}

#[test]
fn toggle_in_a_workspace_with_no_history_is_a_silent_no_op() {
    let dir = temp_state_dir("no-history");
    let output = run_toggle(&dir, "wA", "wA:t1", &["wA:t1"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stderr(&output).is_empty());
}

#[test]
fn toggle_with_a_closed_remembered_tab_is_a_silent_no_op() {
    let dir = temp_state_dir("closed-remembered-tab");
    record_focus(&dir, "wA", "wA:t1", &["wA:t1", "wA:t2"]);
    record_focus(&dir, "wA", "wA:t2", &["wA:t1", "wA:t2"]);
    // wA:t1 has since closed: only t2 remains live.
    let output = run_toggle(&dir, "wA", "wA:t2", &["wA:t2"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stderr(&output).is_empty());
}

#[test]
fn toggle_on_the_already_current_remembered_tab_is_a_silent_no_op() {
    let dir = temp_state_dir("already-current");
    record_focus(&dir, "wA", "wA:t1", &["wA:t1", "wA:t2"]);
    // The snapshot's own focused tab is t1 again, with no distinct target recorded.
    let output = run_toggle(&dir, "wA", "wA:t1", &["wA:t1", "wA:t2"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stderr(&output).is_empty());
}

#[test]
fn tab_focus_failing_with_a_non_tab_not_found_envelope_exits_1_with_its_message() {
    let dir = temp_state_dir("focus-failure");
    record_focus(&dir, "wA", "wA:t1", &["wA:t1", "wA:t2"]);
    record_focus(&dir, "wA", "wA:t2", &["wA:t1", "wA:t2"]);

    let mut cmd = valid_command("toggle", "unused-because-state-dir-is-shared");
    cmd.env("HERDR_PLUGIN_STATE_DIR", &dir);
    cmd.env(
        "FAKE_HERDR_SNAPSHOT_JSON",
        snapshot_json("wA", "wA:t2", &["wA:t1", "wA:t2"]),
    );
    cmd.env("FAKE_HERDR_FOCUS_RESULT", "permission_denied:not allowed");
    let output = run(cmd);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stderr(&output).trim(), "not allowed");
}

#[test]
fn reordering_and_renaming_tabs_between_recording_and_toggling_leaves_the_target_unchanged() {
    // FR-004/SC-005: the plugin keys on `tab_id`, never on `number` or `label`, so a snapshot
    // that reorders or renames tabs between the two events changes nothing about the target.
    let dir = temp_state_dir("reorder-rename");
    record_focus(&dir, "wA", "wA:t1", &["wA:t1", "wA:t3"]);
    record_focus(&dir, "wA", "wA:t3", &["wA:t1", "wA:t3"]);

    // The snapshot embeds `label`/`number` fields the tab-level type ignores; reordering the
    // JSON array and attaching different labels/numbers must not perturb which `tab_id` is the
    // target.
    let reordered_snapshot = r#"{"focused_workspace_id":"wA","focused_tab_id":"wA:t3","workspaces":[{"workspace_id":"wA","active_tab_id":"wA:t3"}],"tabs":[{"tab_id":"wA:t3","workspace_id":"wA","label":"renamed-3","number":9},{"tab_id":"wA:t1","workspace_id":"wA","label":"renamed-1","number":1}]}"#;
    let mut cmd = valid_command("toggle", "unused-because-state-dir-is-shared");
    cmd.env("HERDR_PLUGIN_STATE_DIR", &dir);
    cmd.env("FAKE_HERDR_SNAPSHOT_JSON", reordered_snapshot);
    let output = run(cmd);
    assert_eq!(output.status.code(), Some(0));
    assert!(stderr(&output).is_empty());
    let raw = std::fs::read_to_string(dir.join("state.json")).unwrap();
    assert!(raw.contains("\"current_tab_id\": \"wA:t1\""));
}

/// A snapshot naming several workspaces at once, each with its own live tabs - what a real
/// herdr session actually reports on every read, unlike `snapshot_json`'s single-workspace
/// shorthand. `workspaces` is `(workspace_id, active_tab_id, live_tab_ids)` per workspace.
fn multi_workspace_snapshot_json(
    focused_workspace_id: &str,
    focused_tab_id: &str,
    workspaces: &[(&str, &str, &[&str])],
) -> String {
    let workspace_entries: Vec<String> = workspaces
        .iter()
        .map(|(id, active, _)| format!(r#"{{"workspace_id":"{id}","active_tab_id":"{active}"}}"#))
        .collect();
    let tab_entries: Vec<String> = workspaces
        .iter()
        .flat_map(|(id, _, tabs)| {
            tabs.iter()
                .map(move |t| format!(r#"{{"tab_id":"{t}","workspace_id":"{id}"}}"#))
        })
        .collect();
    format!(
        r#"{{"focused_workspace_id":"{focused_workspace_id}","focused_tab_id":"{focused_tab_id}","workspaces":[{}],"tabs":[{}]}}"#,
        workspace_entries.join(","),
        tab_entries.join(",")
    )
}

fn run_with_snapshot(
    state_dir: &PathBuf,
    subcommand: &str,
    event: Option<&str>,
    snapshot: &str,
) -> Output {
    let mut cmd = valid_command(subcommand, "unused-because-state-dir-is-shared");
    cmd.env("HERDR_PLUGIN_STATE_DIR", state_dir);
    cmd.env("FAKE_HERDR_SNAPSHOT_JSON", snapshot);
    if let Some(event) = event {
        cmd.env("HERDR_PLUGIN_EVENT_JSON", event);
    }
    run(cmd)
}

#[test]
fn every_toggle_stays_inside_its_own_workspace_across_three_workspaces() {
    // SC-003: three workspaces, each with its own two-tab history, toggled in turn - no
    // toggle's target or write may cross into another workspace's entry. Every snapshot here
    // reports all three workspaces at once, as a real herdr session would.
    let dir = temp_state_dir("three-workspace-scoping");
    let workspaces: [(&str, &str, &[&str]); 3] = [
        ("wA", "wA:t1", &["wA:t1", "wA:t2"]),
        ("wB", "wB:t1", &["wB:t1", "wB:t2"]),
        ("wC", "wC:t1", &["wC:t1", "wC:t2"]),
    ];

    for (workspace_id, _, tabs) in workspaces {
        for tab_id in tabs {
            let snapshot = multi_workspace_snapshot_json(workspace_id, tab_id, &workspaces);
            let event = tab_focused_event(workspace_id, tab_id);
            let output = run_with_snapshot(&dir, "tab-focused", Some(&event), &snapshot);
            assert_eq!(output.status.code(), Some(0));
        }
    }

    for (workspace_id, _, tabs) in workspaces {
        // Each workspace's current tab is now its second tab (t2); toggling should return t1.
        let snapshot = multi_workspace_snapshot_json(workspace_id, tabs[1], &workspaces);
        let output = run_with_snapshot(&dir, "toggle", None, &snapshot);
        assert_eq!(
            output.status.code(),
            Some(0),
            "toggle in {workspace_id} stderr: {}",
            stderr(&output)
        );
        assert!(stderr(&output).is_empty());
    }

    let raw = std::fs::read_to_string(dir.join("state.json")).unwrap();
    for (workspace_id, _, _) in workspaces {
        let new_current = format!("{workspace_id}:t1");
        let last_tab = format!("{workspace_id}:t2");
        assert!(
            raw.contains(&format!("\"current_tab_id\": \"{new_current}\""))
                && raw.contains(&format!("\"last_tab_id\": \"{last_tab}\"")),
            "expected {workspace_id} to have alternated to t1, full state: {raw}"
        );
    }
}

#[test]
fn a_workspace_closed_without_its_event_reaching_the_plugin_is_still_pruned_on_the_next_invocation()
{
    // FR-008: the case event-based pruning cannot cover on its own - a workspace closes while
    // the plugin was not running, so no `workspace.closed` event is ever delivered, and only
    // the next invocation's repair pass (regardless of which subcommand it is) notices.
    let dir = temp_state_dir("prune-without-event");
    let both: [(&str, &str, &[&str]); 2] = [
        ("wA", "wA:t1", &["wA:t1", "wA:t2"]),
        ("wB", "wB:t1", &["wB:t1"]),
    ];
    for (workspace_id, tab_id) in [("wA", "wA:t1"), ("wA", "wA:t2"), ("wB", "wB:t1")] {
        let snapshot = multi_workspace_snapshot_json(workspace_id, tab_id, &both);
        let event = tab_focused_event(workspace_id, tab_id);
        let output = run_with_snapshot(&dir, "tab-focused", Some(&event), &snapshot);
        assert_eq!(output.status.code(), Some(0));
    }

    let raw_before = std::fs::read_to_string(dir.join("state.json")).unwrap();
    assert!(
        raw_before.contains("wB"),
        "wB must be recorded before it closes"
    );

    // wB has closed with no event delivered - the next snapshot (here, a toggle in wA) simply
    // no longer reports it as a workspace at all.
    let after_close = run_toggle(&dir, "wA", "wA:t2", &["wA:t1", "wA:t2"]);
    assert_eq!(after_close.status.code(), Some(0));

    let raw_after = std::fs::read_to_string(dir.join("state.json")).unwrap();
    assert!(
        !raw_after.contains("wB"),
        "wB's entry must be pruned by repair alone"
    );
}

// --- T037: concurrency, in two parts that prove different things -----------------------------

#[test]
fn one_hundred_overlapping_pairs_never_produce_torn_state() {
    // SC-009, FR-013 - a *durability* assertion: after every forced-overlapping pair of a
    // `tab-focused` invocation and a `toggle`, state.json must parse and every tab it names
    // must be live in the workspace it is filed under. Atomic replacement alone is what this
    // needs, so unlike the edge case below it does not need the fake herdr to track real focus
    // state - only that the file on disk is never torn.
    let dir = temp_state_dir("sc009-durability");
    let live = ["wA:t1", "wA:t2"];
    record_focus(&dir, "wA", "wA:t1", &live);

    for i in 0..100 {
        let focus_event = tab_focused_event("wA", "wA:t2");
        let focus_snapshot = snapshot_json("wA", "wA:t2", &live);
        let mut focus_cmd = valid_command("tab-focused", &format!("sc009-focus-{i}"));
        focus_cmd.env("HERDR_PLUGIN_STATE_DIR", &dir);
        focus_cmd.env("FAKE_HERDR_SNAPSHOT_JSON", &focus_snapshot);
        focus_cmd.env("HERDR_PLUGIN_EVENT_JSON", &focus_event);

        let toggle_snapshot = snapshot_json("wA", "wA:t1", &live);
        let mut toggle_cmd = valid_command("toggle", &format!("sc009-toggle-{i}"));
        toggle_cmd.env("HERDR_PLUGIN_STATE_DIR", &dir);
        toggle_cmd.env("FAKE_HERDR_SNAPSHOT_JSON", &toggle_snapshot);

        // Spawn both before waiting on either, so the two invocations are genuinely
        // in flight at once rather than run one after the other.
        let focus_child = focus_cmd.spawn().expect("spawn tab-focused");
        let toggle_child = toggle_cmd.spawn().expect("spawn toggle");
        let focus_out = focus_child.wait_with_output().expect("wait tab-focused");
        let toggle_out = toggle_child.wait_with_output().expect("wait toggle");
        assert_eq!(
            focus_out.status.code(),
            Some(0),
            "pair {i} tab-focused: {}",
            stderr(&focus_out)
        );
        assert_eq!(
            toggle_out.status.code(),
            Some(0),
            "pair {i} toggle: {}",
            stderr(&toggle_out)
        );

        let raw = std::fs::read_to_string(dir.join("state.json"))
            .unwrap_or_else(|e| panic!("pair {i}: state.json unreadable: {e}"));
        let parsed: serde_json::Value = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("pair {i}: state.json did not parse: {e}\n{raw}"));
        let entry = parsed
            .get("workspaces")
            .and_then(|w| w.get("wA"))
            .unwrap_or_else(|| panic!("pair {i}: wA entry missing: {raw}"));
        let current = entry["current_tab_id"].as_str().unwrap();
        assert!(
            live.contains(&current),
            "pair {i}: current {current} not live: {raw}"
        );
        if let Some(last) = entry.get("last_tab_id").and_then(|v| v.as_str()) {
            assert!(
                live.contains(&last),
                "pair {i}: last {last} not live: {raw}"
            );
        }
    }
}

#[test]
fn two_concurrent_toggles_in_the_same_workspace_focus_different_tabs_and_return_to_the_start() {
    // The edge case: locking only around the read and the write (rather than spanning `tab
    // focus` too) would let both toggles resolve against the same starting state, both call
    // `tab focus` on the same tab, and both write the same swap - well-formed, live, and wrong,
    // because the user pressed twice and never came back. Asserting only that state.json
    // parses and names a live tab (SC-009's assertion) is satisfied by exactly that broken
    // interleaving, so this test looks at the sequence of `tab focus` arguments instead.
    let dir = temp_state_dir("concurrent-toggle-race");
    let live = ["wA:t1", "wA:t2"];
    record_focus(&dir, "wA", "wA:t1", &live);
    record_focus(&dir, "wA", "wA:t2", &live);
    let starting_state = std::fs::read_to_string(dir.join("state.json")).unwrap();

    // Seeds the fake herdr's tracked focus with the pre-race value, so both processes' first
    // `api snapshot` - whichever runs first - correctly reports t2 as focused, and whichever
    // runs second correctly observes whatever the first one's `tab focus` actually landed on.
    let focus_state_file = dir.join("focused-tab");
    std::fs::write(&focus_state_file, "wA:t2").unwrap();
    let focus_log = dir.join("focus.log");

    // A snapshot naming both tabs; FAKE_HERDR_FOCUS_STATE_FILE overrides its focused_tab_id at
    // read time, so the literal value here only has to be *a* live tab.
    let snapshot = snapshot_json("wA", "wA:t2", &live);

    let make_command = || {
        let mut cmd = valid_command("toggle", "race-unused");
        cmd.env("HERDR_PLUGIN_STATE_DIR", &dir);
        cmd.env("FAKE_HERDR_SNAPSHOT_JSON", &snapshot);
        cmd.env("FAKE_HERDR_FOCUS_STATE_FILE", &focus_state_file);
        cmd.env("FAKE_HERDR_FOCUS_LOG", &focus_log);
        // Widens the race window inside `tab focus` itself - not a rendezvous between the two
        // processes under test, which would deadlock a *correct* implementation: the second
        // toggle blocks on the state lock before it ever reaches herdr.
        cmd.env("FAKE_HERDR_FOCUS_DELAY", "0.2");
        cmd
    };

    let child_a = make_command().spawn().expect("spawn toggle A");
    let child_b = make_command().spawn().expect("spawn toggle B");
    let out_a = child_a.wait_with_output().expect("wait toggle A");
    let out_b = child_b.wait_with_output().expect("wait toggle B");
    assert_eq!(out_a.status.code(), Some(0), "toggle A: {}", stderr(&out_a));
    assert_eq!(out_b.status.code(), Some(0), "toggle B: {}", stderr(&out_b));

    let log = std::fs::read_to_string(&focus_log).unwrap();
    let calls: Vec<&str> = log.lines().collect();
    assert_eq!(
        calls.len(),
        2,
        "each toggle calls tab focus exactly once: {calls:?}"
    );
    assert_ne!(
        calls[0], calls[1],
        "two overlapping toggles must focus different tabs, not the same one twice: {calls:?}"
    );

    let ending_state = std::fs::read_to_string(dir.join("state.json")).unwrap();
    assert_eq!(
        ending_state, starting_state,
        "two toggles must return exactly to the state they started from"
    );
}

// --- T040: the manifest is a public contract (FR-016, FR-019) --------------------------------

mod manifest_contract {
    use serde::Deserialize;
    use std::path::PathBuf;

    #[derive(Deserialize)]
    struct Manifest {
        id: String,
        actions: Vec<Action>,
        events: Vec<Event>,
    }

    #[derive(Deserialize)]
    struct Action {
        id: String,
        command: Vec<String>,
    }

    #[derive(Deserialize)]
    struct Event {
        command: Vec<String>,
    }

    fn read_manifest() -> Manifest {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("herdr-plugin.toml");
        let raw = std::fs::read_to_string(&path).expect("herdr-plugin.toml must exist");
        toml::from_str(&raw).expect("herdr-plugin.toml must parse")
    }

    #[test]
    fn the_qualified_action_id_matches_what_the_readme_and_fr_016_promise() {
        let manifest = read_manifest();
        assert_eq!(manifest.id, "quantumdancer.last-tab");
        assert_eq!(manifest.actions.len(), 1);
        assert_eq!(manifest.actions[0].id, "toggle");
    }

    #[test]
    fn every_dispatched_subcommand_is_named_by_exactly_one_manifest_entry() {
        // src/main.rs dispatches these four; a manifest entry naming anything else, or missing
        // one of these, would drift silently from the binary's actual subcommands without this
        // test catching it in review.
        let manifest = read_manifest();
        let mut command_last_args: Vec<String> = manifest
            .actions
            .iter()
            .map(|action| action.command.clone())
            .chain(manifest.events.iter().map(|event| event.command.clone()))
            .map(|command| command.last().cloned().expect("command must not be empty"))
            .collect();
        command_last_args.sort();

        let mut expected = vec![
            "toggle".to_string(),
            "tab-focused".to_string(),
            "tab-closed".to_string(),
            "workspace-closed".to_string(),
        ];
        expected.sort();

        assert_eq!(command_last_args, expected);
    }
}

#[test]
fn every_subcommand_succeeds_against_a_working_fake_herdr() {
    // T016's stubs take one snapshot and return `Ok(())`, so every subcommand should exit `0`
    // silently against a fake herdr that answers `api snapshot` successfully.
    for subcommand in ["toggle", "tab-focused", "tab-closed", "workspace-closed"] {
        let cmd = valid_command(subcommand, &format!("stub-ok-{subcommand}"));
        let output = run(cmd);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{subcommand} stderr: {}",
            stderr(&output)
        );
        assert!(
            stderr(&output).is_empty(),
            "{subcommand} stderr should be empty"
        );
    }
}
