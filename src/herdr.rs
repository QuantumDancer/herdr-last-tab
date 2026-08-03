//! The seam between this plugin and the herdr binary (Principle I: herdr is the whole API).
//!
//! Everything the plugin knows about live state comes from one `Herdr::snapshot`, and the
//! only mutation it ever performs is `Herdr::focus_tab`. Routing both through a trait — rather
//! than shelling out inline wherever a command body needs them — is what lets every branch of
//! the toggle logic in `src/lib.rs` be exercised with a substituted `Herdr` and no live herdr
//! process, per research.md's "the herdr seam".

use serde::Deserialize;
use std::process::Command;

/// One consistent view of herdr's session state, as returned by `api snapshot`.
///
/// `focused_workspace_id` and `focused_tab_id` are **independently nullable** in herdr's
/// schema — herdr does not promise they come as a pair, so neither `Option` implies the
/// other. `workspaces` and `tabs` carry more fields than this struct names (`agents`, `panes`,
/// `layouts`, `number`, `label`); deriving a plain `Deserialize` without `deny_unknown_fields`
/// ignores them, which is what makes FR-004 (a remembered tab surviving a reorder or rename)
/// hold by construction rather than by code that has to remember not to look at `number` or
/// `label`.
#[derive(Debug, Clone, Deserialize)]
pub struct Snapshot {
    pub focused_workspace_id: Option<String>,
    pub focused_tab_id: Option<String>,
    pub workspaces: Vec<WorkspaceEntry>,
    pub tabs: Vec<TabEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceEntry {
    pub workspace_id: String,
    pub active_tab_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TabEntry {
    pub tab_id: String,
    pub workspace_id: String,
}

/// The three outcomes a herdr call can have, lifted into the type system so no call site can
/// conflate a dead target with an unreachable herdr — see
/// contracts/cli.md#distinguishing-a-dead-target-from-an-unreachable-herdr for the table this
/// enum encodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HerdrError {
    /// `tab focus` answered with code `tab_not_found`: the target is gone. The only
    /// classification a command body may treat as a no-op.
    TabNotFound,
    /// herdr answered with a parseable envelope carrying any other code (including every code
    /// from `api snapshot`, which has no not-found case of its own). Carries the envelope's
    /// message verbatim, since that message is what the FR-010 stderr contract requires.
    Reported(String),
    /// herdr could not be reached at all: the process could not be spawned, produced no
    /// parseable output, or produced JSON that is neither a result nor an error envelope. This
    /// and `Reported` both exit `1`; only `TabNotFound` is a no-op.
    Unreachable(String),
}

/// The two herdr operations the plugin performs, kept as a trait so tests can substitute a
/// fake and prove every branch without a live herdr process (Principle IV).
pub trait Herdr {
    fn snapshot(&self) -> Result<Snapshot, HerdrError>;
    fn focus_tab(&self, tab_id: &str) -> Result<(), HerdrError>;
}

/// The real `Herdr`: invokes `$HERDR_BIN_PATH`, never a bare `herdr` off `PATH` (Principle I —
/// the bin path is handed in rather than looked up here, so there is exactly one place in the
/// whole crate where it is resolved).
pub struct CliHerdr {
    bin_path: String,
}

impl CliHerdr {
    pub fn new(bin_path: impl Into<String>) -> Self {
        Self {
            bin_path: bin_path.into(),
        }
    }

    fn run(&self, args: &[&str]) -> Result<std::process::Output, HerdrError> {
        Command::new(&self.bin_path)
            .args(args)
            .output()
            .map_err(|err| {
                HerdrError::Unreachable(format!("failed to run {}: {err}", self.bin_path))
            })
    }
}

impl Herdr for CliHerdr {
    fn snapshot(&self) -> Result<Snapshot, HerdrError> {
        let output = self.run(&["api", "snapshot"])?;
        if output.status.success() {
            let response: SnapshotResponse =
                serde_json::from_slice(&output.stdout).map_err(|_| unreachable_response())?;
            Ok(response.result.snapshot)
        } else {
            // `api snapshot` takes no target, so it has no not-found case: every envelope it
            // can produce is a genuine failure, never a no-op (contracts/herdr-cli.md).
            Err(classify_failure(&output.stderr, false))
        }
    }

    fn focus_tab(&self, tab_id: &str) -> Result<(), HerdrError> {
        let output = self.run(&["tab", "focus", tab_id])?;
        if output.status.success() {
            Ok(())
        } else {
            Err(classify_failure(&output.stderr, true))
        }
    }
}

fn unreachable_response() -> HerdrError {
    HerdrError::Unreachable("herdr's response could not be parsed".to_string())
}

/// The wrapper `api snapshot` returns on success: `{"result":{"snapshot":{...}}}`.
#[derive(Debug, Deserialize)]
struct SnapshotResponse {
    result: SnapshotResult,
}

#[derive(Debug, Deserialize)]
struct SnapshotResult {
    snapshot: Snapshot,
}

/// The envelope a failing herdr call writes to stderr: `{"error":{"code":...,"message":...}}`.
#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    error: ErrorDetail,
}

#[derive(Debug, Deserialize)]
struct ErrorDetail {
    code: String,
    message: String,
}

/// Classification is by error **code**, against a closed list — not by whether the stderr
/// happens to parse. A parseable envelope proves herdr answered; it does not prove the answer
/// was "that tab is gone". Only code `tab_not_found`, and only when the call was `tab focus`
/// (`is_tab_focus`), maps to the no-op variant. Any other code — an unrecognised one included —
/// is `Reported`, so a herdr that grows a new error this plugin has never seen makes noise
/// rather than silently doing nothing. Stderr that isn't a parseable envelope at all means
/// herdr could not be reached.
fn classify_failure(stderr: &[u8], is_tab_focus: bool) -> HerdrError {
    match serde_json::from_slice::<ErrorEnvelope>(stderr) {
        Ok(envelope) if is_tab_focus && envelope.error.code == "tab_not_found" => {
            HerdrError::TabNotFound
        }
        Ok(envelope) => HerdrError::Reported(envelope.error.message),
        Err(_) => unreachable_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(code: &str, message: &str) -> Vec<u8> {
        format!(r#"{{"error":{{"code":"{code}","message":"{message}"}},"id":"cli:tab:focus"}}"#)
            .into_bytes()
    }

    #[test]
    fn tab_not_found_from_tab_focus_is_classified_as_tab_not_found() {
        let stderr = envelope("tab_not_found", "tab wA:t99 not found");
        assert_eq!(classify_failure(&stderr, true), HerdrError::TabNotFound);
    }

    #[test]
    fn any_other_code_from_tab_focus_is_reported_with_its_message() {
        let stderr = envelope("permission_denied", "not allowed");
        assert_eq!(
            classify_failure(&stderr, true),
            HerdrError::Reported("not allowed".to_string())
        );
    }

    // The case that distinguishes classification-by-code from classification-by-parseability:
    // a parseable `tab_not_found` envelope from `api snapshot` is still a genuine failure,
    // because snapshot has no target and therefore no not-found case of its own.
    #[test]
    fn tab_not_found_from_snapshot_is_reported_not_a_no_op() {
        let stderr = envelope("tab_not_found", "tab wA:t99 not found");
        assert_eq!(
            classify_failure(&stderr, false),
            HerdrError::Reported("tab wA:t99 not found".to_string())
        );
    }

    #[test]
    fn empty_output_is_unreachable() {
        assert_eq!(classify_failure(&[], true), unreachable_response());
        assert_eq!(classify_failure(&[], false), unreachable_response());
    }

    #[test]
    fn non_json_output_is_unreachable() {
        let stderr = b"connection refused".to_vec();
        assert_eq!(classify_failure(&stderr, true), unreachable_response());
    }

    #[test]
    fn json_that_is_neither_result_nor_envelope_is_unreachable() {
        let stderr = br#"{"unexpected":"shape"}"#.to_vec();
        assert_eq!(classify_failure(&stderr, true), unreachable_response());
    }

    #[test]
    fn snapshot_ignores_unknown_fields() {
        let json = r#"{"result":{"snapshot":{
            "focused_workspace_id":"wA","focused_tab_id":"wA:t8",
            "workspaces":[{"workspace_id":"wA","active_tab_id":"wA:t8","label":"x","number":1}],
            "tabs":[{"tab_id":"wA:t8","workspace_id":"wA","label":"2","number":8}],
            "agents":[],"panes":[],"layouts":[]},
            "protocol":17,"version":"0.7.5"}}"#;
        let response: SnapshotResponse = serde_json::from_str(json).expect("parses");
        assert_eq!(
            response.result.snapshot.focused_workspace_id.as_deref(),
            Some("wA")
        );
        assert_eq!(response.result.snapshot.tabs.len(), 1);
    }

    #[test]
    fn snapshot_focus_fields_are_independently_nullable() {
        let json = r#"{"result":{"snapshot":{
            "focused_workspace_id":null,"focused_tab_id":"wA:t8",
            "workspaces":[],"tabs":[]}}}"#;
        let response: SnapshotResponse = serde_json::from_str(json).expect("parses");
        assert_eq!(response.result.snapshot.focused_workspace_id, None);
        assert_eq!(
            response.result.snapshot.focused_tab_id.as_deref(),
            Some("wA:t8")
        );
    }
}
