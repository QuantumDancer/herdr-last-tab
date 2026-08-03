// Dispatch per contracts/cli.md#exit-codes: `0` and silent for success, `1` with an actionable
// message naming what was missing, `2` with a usage message for an unknown or missing
// subcommand.
//
// Subcommand validity is checked *before* the required-environment reads below, even though
// contracts/cli.md lists the `1`-exit environment failures ahead of the `2`-exit usage failure.
// A wiring mistake in the manifest — an unknown or missing subcommand — is a mistake in how
// herdr was told to invoke this binary, independent of whatever environment herdr happened to
// set up around it; checking it first is also what keeps the usage exit reachable without an
// environment configured at all, which is exactly how T020 exercises it.
use herdr_last_tab::herdr::CliHerdr;
use herdr_last_tab::{required_env, tab_closed, tab_focused, toggle, workspace_closed};
use std::path::Path;
use std::process::ExitCode;

const SUBCOMMANDS: &[&str] = &["toggle", "tab-focused", "tab-closed", "workspace-closed"];
const USAGE: &str = "usage: herdr-last-tab <toggle|tab-focused|tab-closed|workspace-closed>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let subcommand = match args.get(1) {
        Some(s) if SUBCOMMANDS.contains(&s.as_str()) => s.as_str(),
        _ => return usage(),
    };

    // `HERDR_BIN_PATH` and `HERDR_PLUGIN_STATE_DIR` are required by every subcommand; the two
    // hook-only inputs below are read defensively, since absent or malformed is their normal
    // state on a plain `toggle` invocation.
    let bin_path = match required_env("HERDR_BIN_PATH") {
        Ok(value) => value,
        Err(message) => return fail(&message),
    };
    let state_dir = match required_env("HERDR_PLUGIN_STATE_DIR") {
        Ok(value) => value,
        Err(message) => return fail(&message),
    };

    let herdr = CliHerdr::new(bin_path);
    let state_dir = Path::new(&state_dir);
    let event_json = std::env::var("HERDR_PLUGIN_EVENT_JSON").ok();
    let context_json = std::env::var("HERDR_PLUGIN_CONTEXT_JSON").ok();

    let result = match subcommand {
        "toggle" => toggle(&herdr, state_dir, context_json.as_deref()),
        "tab-focused" => tab_focused(&herdr, state_dir, event_json.as_deref()),
        "tab-closed" => tab_closed(&herdr, state_dir, event_json.as_deref()),
        "workspace-closed" => workspace_closed(&herdr, state_dir, event_json.as_deref()),
        _ => unreachable!("subcommand was validated against SUBCOMMANDS above"),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => fail(&message),
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

fn fail(message: &str) -> ExitCode {
    eprintln!("{message}");
    ExitCode::from(1)
}
