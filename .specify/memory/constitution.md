<!--
Sync Impact Report
- Version change: 1.0.0 → 1.1.0 (MINOR: new workflow guidance, no principle redefined)
- Modified principles: none
- Modified sections:
  - Development Workflow: added Conventional Commits requirement and the
    pull-request / CodeRabbit / human-merge gate
  - Governance: review-verifies-compliance bullet now names both review stages
- Added sections: none
- Removed sections: none
- Templates requiring update: none — `.specify/templates/plan-template.md` derives its
  Constitution Check gates from this file at plan time and hardcodes no principle list
- Follow-up TODOs: none

Prior revisions
- none → 1.0.0 (initial ratification): Core Principles I-IV, Packaging Constraints,
  Development Workflow, Governance. Dropped the template's fifth placeholder principle —
  four are the working minimum and a fifth would only restate them.
-->

# Herdr Last Tab Constitution

Herdr Last Tab is a [herdr](https://herdr.dev) plugin that toggles focus between the current tab
and the previously focused tab. It is a small, single-purpose binary invoked by herdr as an action
and as event hooks. These principles exist because that shape — an external process herdr spawns,
with no supervisor and no UI of its own — is unforgiving of the failure modes below.

## Core Principles

### I. Herdr Is The Whole API

The plugin reaches herdr only through the documented plugin contract: the herdr CLI invoked via
`$HERDR_BIN_PATH`, and the injected environment (`HERDR_PLUGIN_EVENT_JSON`,
`HERDR_PLUGIN_CONTEXT_JSON`, `HERDR_PLUGIN_STATE_DIR`, `HERDR_TAB_ID`, and peers).

- The plugin MUST invoke herdr through `$HERDR_BIN_PATH`, never a bare `herdr` on `PATH`.
- The plugin MUST NOT read or write herdr's config, session files, database, or socket directly,
  and MUST NOT depend on herdr internals that the CLI and event payloads do not expose.
- Every environment variable and JSON payload MUST be parsed defensively: absent, empty, and
  malformed inputs are expected states, not panics.
- `min_herdr_version` MUST name the oldest release that actually provides every CLI subcommand,
  event, and payload field the plugin relies on. Raising a dependency on a newer field raises the
  floor in the same change.

Rationale: herdr states plainly that the entire CLI is the plugin API and that there is no SDK.
Anything reached around that boundary is an undeclared dependency on an internal that will move,
and it will move without a version signal the plugin can check.

### II. Stable Identity, Owned State

The plugin tracks tabs by herdr's stable tab ID, and keeps everything it remembers inside the
directory herdr gave it.

- Focus history MUST be keyed by tab ID. Tab index, position, ordinal, title, and cwd MUST NOT be
  used to identify a remembered tab; reordering and renaming tabs MUST NOT change the target.
- All durable state MUST live under `HERDR_PLUGIN_STATE_DIR` (user-editable settings, if any, under
  `HERDR_PLUGIN_CONFIG_DIR`). The plugin MUST NOT write anywhere else — not `HERDR_PLUGIN_ROOT`,
  not the user's repo, not `$HOME`.
- State writes MUST be atomic (write-temp-then-rename) and MUST be serialized by a lock, because
  the action and the event hooks are separate processes that can run concurrently.
- Unreadable or malformed state MUST be treated as empty state and repaired in place. A corrupt
  state file is a recoverable condition, never a crash.
- State MUST NOT be left pointing at a tab that no longer exists; close events and toggle-time
  validation both prune stale targets.

Rationale: positions renumber and titles change while IDs do not, so identity is the only durable
key. And because herdr spawns each hook as its own short-lived process with no ordering guarantee,
concurrency safety and corruption tolerance are properties of the file format, not of the code path
that happens to run first.

### III. Quiet No-Ops, Loud Failures

The plugin distinguishes "nothing to do" from "I am broken", and herdr's user only hears about the
second.

- Every expected no-op MUST exit `0` with no error output: no history recorded yet, the remembered
  tab was closed, the remembered tab is already focused, herdr reports no focused tab.
- A non-zero exit is reserved for the plugin being unable to run or unable to talk to herdr, and
  MUST be accompanied by an actionable message on stderr naming what was missing.
- Events MUST be validated against herdr's current state before they are trusted. An event that no
  longer reflects reality is discarded, not applied.

Rationale: the plugin is bound to a keystroke the user presses constantly. Herdr surfaces a failing
command as a toast, so any no-op that exits non-zero becomes a recurring, meaningless interruption —
and once the user learns to ignore those toasts, the real failures are invisible too. Stale events
are the same bargain: applying one silently corrupts the history that the whole feature is.

### IV. Verifiable Without Herdr Running

Behavior is proven by tests that run in CI, on a machine with no herdr and no terminal.

- State transitions and toggle target selection MUST be exercised by automated tests against
  recorded herdr CLI output and synthetic payloads. Herdr invocations MUST sit behind a seam that
  tests can substitute.
- Every principle above has a test that fails when it is violated: stale-event rejection, corrupt
  state repair, concurrent writes, exit code per no-op case, and no writes outside the state dir.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` MUST pass
  before a change is complete. A change that cannot be verified this way is not done.
- Manual verification in a real herdr session is required for changes to the manifest,
  the action or event wiring, and the install path — automated tests do not cover that seam.

Rationale: the interactive path needs a running herdr, several live tabs, and a human pressing a
key, which makes it far too expensive to be the primary check. Pushing every state decision behind
a seam is what keeps the fast check meaningful, and it leaves manual QA responsible only for the
wiring that automation genuinely cannot reach.

## Packaging Constraints

- `herdr-plugin.toml` is the plugin's public contract. Action IDs and their qualified form
  (`<plugin.id>.<action.id>`) are what users bind keys to; renaming one is a breaking change and
  MUST come with a major version bump and a README migration note.
- Declared `platforms` MUST be limited to what is actually tested. An untested platform is not a
  supported one.
- Any toolchain the build step requires MUST be stated in the README, because
  `herdr plugin install` runs that build on the user's machine.
- The README MUST carry a working `[[keys.command]]` example and the install, link, and uninstall
  commands. A plugin action nobody can bind is not shipped.

## Development Workflow

- Behavior changes update the README in the same change. Documented behavior and actual behavior
  diverging is a defect, reported rather than silently patched.
- Deferred work, incidental findings, and known gaps are logged in `SESSION.local.md`, not fixed
  opportunistically inside unrelated changes.
- Local development uses `herdr plugin link` against the checkout. A locally linked build MUST NOT
  be swapped into a GitHub-installed plugin root by hand.
- Commit messages MUST follow [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/):
  a `<type>[(<scope>)]: <description>` subject, plus a body explaining any non-obvious design or
  implementation trade-off. A breaking change MUST carry `!` before the colon and a
  `BREAKING CHANGE:` footer naming what breaks — a renamed action ID under Packaging Constraints is
  exactly that, and lands with the major version bump and README migration note in the same change.
- Every feature MUST land through a pull request. Direct pushes to the default branch are not part
  of this workflow, including for changes an author considers trivial.
- Each pull request MUST be reviewed by CodeRabbit before it reaches a human. Every CodeRabbit
  finding is fixed or answered in the thread; leaving one silently open blocks the human review.
- A human review follows the automated one, and a human performs the merge. An agent MUST NOT merge
  a pull request, approve its own change, or bypass a required review — a green check is evidence
  for the human decision, never a substitute for it.

## Governance

This constitution supersedes ad-hoc practice for this repository. Where a pull request, an issue, or
an agent instruction conflicts with it, this document wins until it is amended.

- Amendments are made by editing this file in a dedicated change that states the rationale, updates
  the version, and refreshes the Sync Impact Report.
- Versioning is semantic: MAJOR for removing or redefining a principle in a way that invalidates
  existing practice, MINOR for adding a principle or materially expanding guidance, PATCH for
  clarifications and wording.
- Every review verifies compliance — the automated pass and the human one both. A change that
  violates a principle is either revised or accompanied by an amendment — never merged as a silent
  exception.
- Complexity is justified in the change description or it is removed. This plugin is small on
  purpose; the burden of proof sits with the addition.

**Version**: 1.1.0 | **Ratified**: 2026-08-02 | **Last Amended**: 2026-08-02
