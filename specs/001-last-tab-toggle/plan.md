# Implementation Plan: Per-Workspace Last Tab Toggle

**Branch**: `001-last-tab-toggle` | **Date**: 2026-08-02 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-last-tab-toggle/spec.md`

## Summary

A single small Rust binary that herdr spawns as one action and three event hooks. It remembers, per
workspace, the currently focused tab and the one focused before it, and the action focuses the
latter. History lives in one JSON file under the directory herdr designates, keyed by workspace ID
and holding tab IDs, guarded by an exclusive lock and replaced atomically because herdr spawns each
invocation as its own short-lived process.

The whole design turns on one rule: an observation naming the tab a workspace *already* holds as
current records no transition. That single rule is what keeps workspace activation from seeding
history, what keeps one workspace's activity out of another's, and what lets the toggle write its own
swap without fighting the event hook that follows it.

Everything the plugin needs from herdr is re-read on every invocation and validated before it is
acted on. Nothing remembered is trusted.

## Technical Context

**Language/Version**: Rust 2021, toolchain pinned in `rust-toolchain.toml`

**Primary Dependencies**: `serde` (derive), `serde_json`, `fs2`. No argument parser — four
subcommands with no flags match on `args().nth(1)`, which is smaller than the dependency and cannot
drift from the manifest.

**Storage**: One JSON file, `$HERDR_PLUGIN_STATE_DIR/state.json`, with `state.lock` beside it. Shape
and invariants in [data-model.md](./data-model.md).

**Testing**: `cargo test`. Unit tests exercise every state transition against a substituted `Herdr`
trait; integration tests spawn the real binary against a fake herdr shell script to prove the process
boundary and the locking. Neither needs herdr installed.

**Target Platform**: `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`,
`x86_64-apple-darwin`, `aarch64-apple-darwin`. musl so the Linux binaries do not pin a host glibc.

**Project Type**: Single-binary CLI plugin. No library consumers, no UI, no server.

**Performance Goals**: SC-002 — the 80th percentile of 20 warm invocations under 150 ms, measured
from invoking the action to herdr reporting the new tab as focused. Baseline host: 13th Gen Intel Core
i9-13900H, 4 cores available, 8 GB RAM, Linux x86_64. Harness in
[quickstart.md](./quickstart.md#sc-002--under-150-ms). The warm path is one read and one write, both
local socket round-trips.

**Constraints**: Every expected no-op exits `0` silently; only an unreachable herdr, an unrecognised
error from it, or a missing required runtime variable exits non-zero. `HERDR_PLUGIN_CONTEXT_JSON` is
not one of those — it is optional, and "no workspace focused" is an exit-`0` no-op. No write outside
`HERDR_PLUGIN_STATE_DIR`. Installation must not require a compiler.

**Scale/Scope**: ~20 stored workspaces × ~10 tabs each — the SC-002 workload. State stays a few
kilobytes, so the file is read and rewritten whole.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Evaluated against [`.specify/memory/constitution.md`](../../.specify/memory/constitution.md) v1.1.0.
**Initial: PASS. Post-design re-check: PASS**, with one justified entry in Complexity Tracking.

### I. Herdr Is The Whole API — PASS

The plugin invokes exactly two commands, both through `$HERDR_BIN_PATH`, never a bare `herdr`:
`api snapshot` for every read and `tab focus <id>` for the one write. The consumed surface is pinned field
by field in [contracts/herdr-cli.md](./contracts/herdr-cli.md). No config file, session file,
database, or socket is touched. Every environment variable and JSON payload is parsed defensively —
`HERDR_PLUGIN_CONTEXT_JSON` in particular, because every field in herdr's schema for it is optional,
so it is a hint cross-checked against live state rather than an input.

`min_herdr_version = "0.7.5"` is the version this plugin has been exercised against, and
`contracts/herdr-cli.md` is the enumeration the constitution asks the floor to be derived from.
Lowering it is deferred testing work, logged in `SESSION.local.md`.

### II. Stable Identity, Owned State — PASS

History is keyed by `workspace_id` and stores `tab_id` values. `number` and `label` — the fields that
change on reorder and rename — appear in the snapshot only to be ignored, which is why SC-005 holds
by construction. All durable state is under `HERDR_PLUGIN_STATE_DIR`; the plugin ships no settings,
so it never writes the config directory, and it never writes the plugin root. Writes are
temp-then-`rename` under an exclusive `fs2` lock held across the whole read-modify-write. Unreadable
state is empty state, repaired by the next write. Stale targets are pruned by both paths the
constitution names: the close hooks, and validation at toggle time.

### III. Quiet No-Ops, Loud Failures — PASS

The exit-code contract is [contracts/cli.md](./contracts/cli.md): `0` and silent for every expected
no-op, `1` with a message naming what was missing when herdr cannot be reached or the runtime context
is absent. The split rests on herdr's error envelope — a parseable envelope means herdr answered, so
a dead tab is a no-op; no envelope means herdr could not be reached, which is the failure. Events are
validated against live state before they are trusted: a `tab.focused` naming a tab herdr does not
currently report as focused is discarded, not applied.

### IV. Verifiable Without Herdr Running — PASS

The `Herdr` trait is the seam; every branch of every subcommand is reachable from a unit test with no
herdr and no terminal. Each principle above has a test that fails when violated — stale-event
rejection, corrupt-state repair, concurrent writes, exit code and stderr per no-op case, and no
writes outside the state directory. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
and `cargo test` gate CI from the first PR. The manifest, action and event wiring, and install path
are exactly what automation cannot reach, and they are covered by [quickstart.md](./quickstart.md) as
the constitution's required manual pass.

### Packaging and workflow constraints — PASS

`herdr-plugin.toml` is treated as a public contract with its own compatibility rules
([contracts/plugin-manifest.md](./contracts/plugin-manifest.md)).

Two related but distinct lists have to stay in step, and the constitution's "declared platforms MUST
be limited to what is actually tested" applies to both:

- The manifest's `platforms` holds **two host-platform values**, `linux` and `macos`. That is the
  vocabulary herdr's manifest parser accepts, and it is what herdr matches against when it raises
  `platform_unsupported`.
- The release matrix builds **four architecture-specific target triples** —
  `{x86_64,aarch64}-unknown-linux-musl` and `{x86_64,aarch64}-apple-darwin`.

They are not the same list at different resolutions: the manifest cannot express an architecture, so
declaring `linux` is a claim about *both* Linux triples. The rule that keeps FR-020 honest is
therefore that a manifest platform may be declared only when **every** triple that platform covers is
built and exercised. Dropping one Linux architecture would mean dropping `linux` from the manifest,
not silently narrowing what `linux` means.

The build step's toolchain requirement is nil by design — it downloads a prebuilt — and the README
carries the `[[keys.command]]` example plus install, link, list, verify, and uninstall. Work lands in
three reviewed pull requests; no agent merges.

## Project Structure

### Documentation (this feature)

```text
specs/001-last-tab-toggle/
├── plan.md              # This file
├── research.md          # Phase 0: verified herdr contract + design decisions
├── data-model.md        # Phase 1: state shape, invariants, transition tables
├── quickstart.md        # Phase 1: runnable validation per success criterion
├── contracts/           # Phase 1
│   ├── plugin-manifest.md   # public: the manifest and the action identifier
│   ├── cli.md               # internal: subcommands and the exit-code contract
│   └── herdr-cli.md         # consumed: the herdr surface, verified against 0.7.5
├── checklists/
│   └── requirements.md
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created here)
```

### Source Code (repository root)

```text
herdr-plugin.toml            # public contract: id, action, events, platforms, min_herdr_version
Cargo.toml
Cargo.lock
rust-toolchain.toml
src/
├── main.rs                  # subcommand dispatch and exit-code mapping
├── lib.rs                   # the four command bodies
├── herdr.rs                 # the Herdr trait seam, CLI invocation, JSON parsing
└── state.rs                 # load, repair, persist; lock and atomic replace
tests/
└── cli.rs                   # spawns the binary against a fake herdr script
herdr/
├── run.sh                   # launcher: prebuilt bin/ else target/release/
└── install.sh               # prebuilt download and checksum verification
.github/workflows/
├── ci.yml                   # fmt, clippy -D warnings, test
└── release.yml              # four targets, sha256 sidecars, draft-then-publish
README.md
CHANGELOG.md
docs/RELEASING.md
```

**Structure Decision**: a single Rust binary at the repository root, which is what herdr's plugin
layout expects — the manifest sits beside the sources it builds. Three modules rather than the one
large `lib.rs` the reference plugin uses: `herdr.rs` and `state.rs` have genuinely separate
responsibilities and separate test suites, while the four command bodies stay sequential and inline
in `lib.rs` rather than being decomposed into a call graph that only reorganizes the file.

### Delivery

Three pull requests, each independently reviewable and each landing through CodeRabbit and a human:

| PR | Content | Establishes |
| --- | --- | --- |
| A · `chore:` | Cargo project, manifest, `herdr/run.sh`, the `Herdr` seam and state module with their tests, `ci.yml` | The gates are green and `herdr plugin action list` shows `quantumdancer.last-tab.toggle` |
| B · `feat:` | Toggle and the three hooks, per-workspace history, the repair pass, README | User Stories 1, 2, 3 — the viable feature |
| C · `ci:` | `herdr/install.sh`, `release.yml`, `CHANGELOG.md`, `docs/RELEASING.md`, `version = "0.1.0"` released as tag `v0.1.0` | User Story 4 |

PR C implements the `v<version>` tag mapping and the checks at both ends that
[contracts/plugin-manifest.md](./contracts/plugin-manifest.md#the-version-to-tag-rule) defines; that
rule is stated once there rather than restated here.

PR B is the first point at which the plugin is worth installing. The spec's Prior Art section is
explicit that User Story 1 alone ships a duplicate of `herdr-recent-navigator`, so the per-workspace
scoping is not a follow-up — it is what makes the release viable, and it ships in the same PR.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| `herdr/run.sh`, a shell launcher between the manifest and the binary | `herdr plugin link` skips the `[[build]]` step that a prebuilt install uses to place the binary in `bin/`. The two supported install paths therefore hold the binary in different places, and one literal path in the manifest cannot serve both. | A literal `./bin/herdr-last-tab` plus a symlink step for development makes `herdr plugin link .` silently produce a broken plugin, failing as a missing action rather than an error. Downloading the prebuilt into `target/release/` so both paths coincide means `cargo clean` deletes an installed user's plugin. |
