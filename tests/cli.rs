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
