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
// Each function below is a Phase 2 stub (T016): it reads what its final implementation will
// need — the `Herdr` seam, the state directory, and whatever payload the subcommand carries —
// and takes exactly one `api snapshot` through that seam before returning `Ok(())`. No state is
// loaded or written yet. Taking the snapshot rather than returning immediately is deliberate:
// it is what puts `CliHerdr` on a real process boundary while the seam is the only thing under
// test, which is the herdr-unreachable path T020 exercises. Phases 3 and 4 fill these bodies in
// (T029-T031, T039) without changing a signature or touching `main.rs`'s dispatch, so every
// parameter a later phase needs is already here even though this phase does nothing with most
// of them yet.
// ---------------------------------------------------------------------------------------------

/// The `toggle` action. `_context_json` is `HERDR_PLUGIN_CONTEXT_JSON`, used later as a fallback
/// for the focused workspace.
pub fn toggle(
    herdr: &dyn Herdr,
    _state_dir: &Path,
    _context_json: Option<&str>,
) -> Result<(), String> {
    herdr.snapshot().map_err(describe)?;
    Ok(())
}

/// The `tab.focused` hook. `_event_json` is `HERDR_PLUGIN_EVENT_JSON`.
pub fn tab_focused(
    herdr: &dyn Herdr,
    _state_dir: &Path,
    _event_json: Option<&str>,
) -> Result<(), String> {
    herdr.snapshot().map_err(describe)?;
    Ok(())
}

/// The `tab.closed` hook.
pub fn tab_closed(
    herdr: &dyn Herdr,
    _state_dir: &Path,
    _event_json: Option<&str>,
) -> Result<(), String> {
    herdr.snapshot().map_err(describe)?;
    Ok(())
}

/// The `workspace.closed` hook.
pub fn workspace_closed(
    herdr: &dyn Herdr,
    _state_dir: &Path,
    _event_json: Option<&str>,
) -> Result<(), String> {
    herdr.snapshot().map_err(describe)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::herdr::{Snapshot, TabEntry, WorkspaceEntry};

    struct FakeHerdr {
        snapshot: Result<Snapshot, HerdrError>,
    }

    impl Herdr for FakeHerdr {
        fn snapshot(&self) -> Result<Snapshot, HerdrError> {
            self.snapshot.clone()
        }

        fn focus_tab(&self, _tab_id: &str) -> Result<(), HerdrError> {
            unreachable!("Phase 2 stubs never call focus_tab")
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
    fn stub_bodies_succeed_on_a_successful_snapshot() {
        let fake = FakeHerdr {
            snapshot: Ok(empty_snapshot()),
        };
        let dir = Path::new("/nonexistent-not-touched-by-a-stub");
        assert!(toggle(&fake, dir, None).is_ok());
        assert!(tab_focused(&fake, dir, None).is_ok());
        assert!(tab_closed(&fake, dir, None).is_ok());
        assert!(workspace_closed(&fake, dir, None).is_ok());
    }

    #[test]
    fn stub_bodies_surface_an_unreachable_herdr() {
        let fake = FakeHerdr {
            snapshot: Err(HerdrError::Unreachable(
                "herdr could not be reached".to_string(),
            )),
        };
        let dir = Path::new("/nonexistent-not-touched-by-a-stub");
        let err = toggle(&fake, dir, None).unwrap_err();
        assert_eq!(err, "herdr could not be reached");
    }
}
