# Tasks: Per-Workspace Last Tab Toggle

**Input**: Design documents from `/specs/001-last-tab-toggle/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: Included and mandatory. Constitution Principle IV requires every principle to have a test
that fails when it is violated, and [contracts/cli.md](./contracts/cli.md#tests-this-contract-owes)
enumerates the tests this contract owes. Test tasks here are not optional additions.

**Organization**: Tasks are grouped by user story so each story can be implemented and tested
independently. The three-PR delivery split from [plan.md](./plan.md#delivery) maps onto the phases:

| PR | Phases |
| --- | --- |
| A · `chore:` | Phase 1 (Setup) + Phase 2 (Foundational) |
| B · `feat:` | Phase 3 (US1) + Phase 4 (US2) + Phase 5 (US3) |
| C · `ci:` | Phase 6 (US4) + Phase 7 (Polish) |

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel — different file, no dependency on an incomplete task
- **[Story]**: US1/US2/US3/US4, mapping the task to a user story in [spec.md](./spec.md)
- A task that **changes a file** names the exact file it touches. A task that only **runs** something
  — the manual quickstart passes T043 and T052-T056, the release cut T051, the final gate pass T058 —
  names the procedure it follows and the artifact it verifies instead, since it produces no diff

## Path Conventions

Single Rust binary at the repository root, per
[plan.md](./plan.md#source-code-repository-root): `src/`, `tests/`, `herdr/`, `.github/workflows/`,
and the manifest beside them. Unit tests live in a `#[cfg(test)] mod tests` inside the module they
exercise, so a test task and the implementation task it covers touch the same file.

**`[P]` therefore means no other task in the same group touches that file** — it is not a claim about
whether the work is conceptually independent. Two tasks against `src/lib.rs` are never both `[P]`
even when they test unrelated command bodies, because the constraint is the file, not the subject.
`tests/cli.rs` is the one file touched from five phases — **T019, T020, T026, T036, T037, T040,
T044** — so every task naming it serializes against the others regardless of which phase they sit in.

Re-derive that list from the tasks rather than trusting this line. Two successive revisions of it
were wrong: the first omitted T040 and thereby re-permitted the parallelism the rule exists to
forbid, and the correction then omitted T020. A hand-maintained membership list for a file this
widely shared is the wrong shape for the constraint. Re-derive it with a pattern anchored to task
declarations, so prose, dependency notes, and this command itself cannot land in the result:

```sh
grep -nE '^- \[[ x]\].*tests/cli\.rs' tasks.md
```

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: A buildable Rust project whose CI gates run green, with the manifest and launcher in
place so `herdr plugin link .` produces a plugin herdr recognises.

- [x] T001 Create `Cargo.toml` — package `herdr-last-tab`, `version = "0.1.0"`, edition 2021, binary and library targets, dependencies `serde` (with `derive`), `serde_json`, `fs2`, plus a `[dev-dependencies]` entry for `toml`. No argument-parsing crate: four subcommands with no flags match on `args().nth(1)` (see [research.md](./research.md#language-and-dependencies-rust-with-serde-serde_json-fs2)). `toml` is a **dev**-dependency and nothing else may be: T040 and T044 read `herdr-plugin.toml` to assert the public action identifier and the version pairing, and hand-matching TOML with a regex is how a contract test comes to pass against a manifest it has stopped understanding. It never links into the shipped binary, so the three runtime dependencies [plan.md](./plan.md#technical-context) names are unchanged
- [x] T002 [P] Create `rust-toolchain.toml` pinning the toolchain channel and the `rustfmt` and `clippy` components, so the CI gates and a local run resolve the same versions
- [x] T003 [P] Extend `.gitignore` with `/target/` and `/bin/` — `bin/` holds the prebuilt binary `herdr/install.sh` downloads and must never be committed
- [x] T004 [P] Create `herdr-plugin.toml` verbatim from [contracts/plugin-manifest.md](./contracts/plugin-manifest.md#the-manifest): `id`, `name`, `version`, `min_herdr_version = "0.7.5"`, `description`, `platforms = ["linux", "macos"]`, the `[[build]]` step, the `toggle` action, and the three `[[events]]` hooks
- [x] T005 [P] Create `herdr/run.sh` — exec `$HERDR_PLUGIN_ROOT/bin/herdr-last-tab "$@"` when present, else `$HERDR_PLUGIN_ROOT/target/release/herdr-last-tab "$@"`; `set -euo pipefail`; passes `shellcheck` clean. This launcher is the Complexity Tracking row in [plan.md](./plan.md#complexity-tracking) — comment it with why one literal manifest path cannot serve both install paths
- [x] T006 [P] Create `.github/workflows/ci.yml` running `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and a shellcheck step on push and pull request. The shellcheck step **enumerates** the scripts that exist rather than globbing paths that do not — `find . -name '*.sh' -not -path './target/*' -print0 | xargs -0 -r shellcheck`. A literal `shellcheck herdr/*.sh tests/*.sh` is red at this very checkpoint: `tests/` does not exist until T019, an unmatched glob is passed through verbatim, and shellcheck exits non-zero on the missing path. A gate that cannot pass on the commit that introduces it teaches everyone to ignore it, and `xargs -r` is what keeps the step green when the enumeration is legitimately empty
- [x] T007 Create a minimal `src/main.rs` that prints the usage message and exits `2`, **and an empty `src/lib.rs`**, so `cargo build` and every gate in T006 succeed before any command body exists (real dispatch arrives in T016, the first module in T015). Both files are needed, not just the binary: T001 declares a library target, and a declared `[lib]` with no `src/lib.rs` fails the build outright — which would leave the checkpoint below unverifiable at exactly the phase whose whole purpose is proving the gates run green

**Checkpoint**: `cargo build --release` succeeds, all four CI gates pass, and
`herdr plugin link . && herdr plugin action list --plugin quantumdancer.last-tab` shows
`quantumdancer.last-tab` / `toggle` — the PR A outcome from [plan.md](./plan.md#delivery).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The two seams every user story is built on — the `Herdr` trait that makes behavior
provable with no herdr running (Principle IV), and the locked, atomic, corruption-tolerant state file
(Principle II). No command body does anything yet; the four exist as stubs from T016 so the crate
builds and the exit-code contract is testable, and Phases 3 and 4 fill them in.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [x] T008 [P] Define the snapshot types in `src/herdr.rs` — `Snapshot { focused_workspace_id: Option<String>, focused_tab_id: Option<String>, workspaces: Vec<WorkspaceEntry>, tabs: Vec<TabEntry> }`, `WorkspaceEntry { workspace_id, active_tab_id }`, `TabEntry { tab_id, workspace_id }`, deriving `Deserialize`. `focused_workspace_id` and `focused_tab_id` are **independently nullable** in herdr's schema ([data-model.md](./data-model.md#state-transitions-per-subcommand)), so both are `Option` and neither implies the other. Unknown fields (`agents`, `panes`, `layouts`, `number`, `label`) are ignored by default, which is what makes FR-004 hold by construction
- [x] T009 [P] Define the persisted types in `src/state.rs` — `PersistedState { workspaces: BTreeMap<String, WorkspaceHistory> }` and `WorkspaceHistory { current_tab_id: String, last_tab_id: Option<String> }`, deriving `Serialize`/`Deserialize`, exactly as in [data-model.md](./data-model.md#persisted-state). Comment why `BTreeMap` rather than `HashMap`: stable key order makes two runs producing the same change byte-identical, which is what lets the SC-009 test compare files
- [x] T010 Define the `Herdr` trait in `src/herdr.rs` — `fn snapshot(&self) -> Result<Snapshot, HerdrError>` and `fn focus_tab(&self, tab_id: &str) -> Result<(), HerdrError>`, plus `enum HerdrError { TabNotFound, Reported(String), Unreachable(String) }`. The three variants are the classification table in [contracts/cli.md](./contracts/cli.md#distinguishing-a-dead-target-from-an-unreachable-herdr), lifted into the type system so no call site can conflate a dead target with an unreachable herdr (depends on T008)
- [x] T011 Implement `CliHerdr` in `src/herdr.rs` — invokes `$HERDR_BIN_PATH api snapshot` and `$HERDR_BIN_PATH tab focus <id>`, never a bare `herdr` (Principle I). Parse the `{"result":{"snapshot":{…}}}` wrapper on success; on failure parse the stderr envelope and map code `tab_not_found` from `tab focus` to `TabNotFound`, **any other code** to `Reported(message)`, and a spawn failure, empty output, or output that parses as neither result nor envelope to `Unreachable`. `api snapshot` has no not-found case, so any envelope from it is `Reported` (depends on T010)
- [x] T012 Implement state loading in `src/state.rs` — `load(dir) -> PersistedState` returning an empty map for a missing, empty, truncated, or malformed file. Never an error, never a panic: unreadable content is empty content (FR-014, invariant 5) (depends on T009)
- [x] T013 Implement the lock guard in `src/state.rs` — acquire an exclusive `fs2` lock on `$HERDR_PLUGIN_STATE_DIR/state.lock`, creating the directory and file if absent, and hold it across the entire read-modify-write including the `tab focus` call. Comment the deliberate cost: serializing the herdr round-trip is what stops two overlapping toggles from both resolving the same target ([data-model.md](./data-model.md#concurrency)) (depends on T012)
- [x] T014 Implement `persist` in `src/state.rs` — serialize, write to a uniquely named temporary file in the same directory, `sync_all`, then `rename` over `state.json`; release the lock by dropping the guard. Same-directory `rename` is atomic, so a concurrent reader sees the old file or the new one and never a partial write (depends on T013)
- [x] T015 Implement environment access in `src/lib.rs` — `HERDR_BIN_PATH` and `HERDR_PLUGIN_STATE_DIR` are required and an absent **or empty** value is the FR-010 failure naming the variable; `HERDR_PLUGIN_EVENT_JSON` and `HERDR_PLUGIN_CONTEXT_JSON` are parsed defensively where absent, empty, and malformed are expected states. The event parser reads `data.<field>` and falls back to a top-level `<field>` ([contracts/herdr-cli.md](./contracts/herdr-cli.md#events)); the context parser exposes only `workspace_id`, since every field in herdr's schema is optional (depends on T010)
- [x] T016 Replace the T007 placeholder in `src/main.rs` with the real dispatch — match `args().nth(1)` on `toggle`, `tab-focused`, `tab-closed`, `workspace-closed`, delegating to four command-body functions in `src/lib.rs`, with exit-code mapping per [contracts/cli.md](./contracts/cli.md#exit-codes): `0` and silent for success and every no-op, `1` with an actionable stderr message naming what was missing, `2` with a usage message for an unknown or missing subcommand. **This task declares those four functions as stubs**: each reads the environment (T015), takes one `api snapshot` through the `Herdr` seam, and returns `Ok(())` without loading or writing state. T029, T030, T031, and T039 fill the stubs in without changing a signature or touching the dispatch. The stubs are what make Phase 2 compile and its checkpoint mean something — the alternative, dispatching to bodies that arrive two phases later, leaves the crate unbuildable for the whole of Phase 2 and makes T020 unrunnable. Taking the snapshot rather than returning immediately is deliberate: it is what puts `CliHerdr` on a real process boundary while the seam is the only thing under test, which is exactly the herdr-unreachable path T020 exercises (depends on T015)
- [x] T017 [P] Unit-test state persistence in `src/state.rs` — a missing file, an empty file, `not json`, and a file truncated mid-object each load as an empty map and are repaired by the next write; a `persist` produces valid JSON with stable key order; no file is created outside the state directory (FR-014, FR-015, SC-007) (depends on T014)
- [x] T018 [P] Unit-test envelope classification in `src/herdr.rs` — a `tab_not_found` envelope from `tab focus` yields `TabNotFound`; an envelope with any other code yields `Reported` carrying the envelope's `message`; empty output, non-JSON output, and JSON that is neither result nor envelope each yield `Unreachable`; any envelope from `api snapshot` yields `Reported`. This is the test that distinguishes classification-by-code from classification-by-parseability, without which the contract table is untested (depends on T011)
- [x] T019 [P] Create the integration harness — `tests/fake-herdr.sh`, a shellcheck-clean script that emits a caller-supplied snapshot JSON for `api snapshot` and records or fails `tab focus` per fixture, and `tests/cli.rs`, which spawns the built binary with `HERDR_BIN_PATH` pointed at it and `HERDR_PLUGIN_STATE_DIR` in a temp directory. This is the second layer [research.md](./research.md#the-herdr-seam-a-herdr-trait) calls for: it proves the process boundary that a substituted trait cannot. It does **not** reach the state lock at this phase — T016's stubs take a snapshot and return, touching no state — so locking stays unproven until T037 drives overlapping invocations through this same harness (depends on T016)
- [x] T020 Integration-test the failure exits in `tests/cli.rs` — each of `HERDR_BIN_PATH` and `HERDR_PLUGIN_STATE_DIR` absent, and each present but empty, exits `1` with a stderr message naming that variable; a herdr that cannot be spawned exits `1`; an unparseable herdr response exits `1`; an unknown subcommand and a missing subcommand each exit `2` with a usage message (FR-010, [contracts/cli.md](./contracts/cli.md#tests-this-contract-owes)). Every one of these is reachable against T016's stubs, because each aborts before a command body would do anything: the environment failures in T015's parsing, the usage failures in the dispatch, and the herdr failures in the stub's own snapshot call. The remaining `1`-exit row — a `tab focus` failing with a non-`tab_not_found` envelope — needs a real toggle and lands in T026 (depends on T019)

**Checkpoint**: Both seams exist and are tested. State survives corruption, the herdr classification
is provable without herdr, and the binary's failure exits are pinned. User story work can begin.

---

## Phase 3: User Story 1 - Jump back to the tab I was just in (Priority: P1) 🎯 MVP

**Goal**: The toggle moves focus to the previously focused tab and back again, driven by history that
the `tab.focused` hook records and that repairs itself rather than acting on a dead target.

**Independent Test**: Open a workspace with three tabs, focus tab 1, focus tab 3, invoke the toggle,
confirm tab 1 is focused. Invoke again and confirm tab 3 is focused.

**⚠️ Not shippable alone.** [spec.md](./spec.md#prior-art) is explicit that US1 by itself reproduces
`herdr-recent-navigator`. Phase 4 is what makes the release worth installing, and both ship in PR B.

### Tests for User Story 1

> Write these first and confirm they fail before the implementation task they name.

- [x] T021 [US1] Unit-test the tab-level repair steps in `src/state.rs` — a `current_tab_id` absent from the snapshot is replaced by that workspace's `active_tab_id`; an entry whose workspace has no live tabs at all is dropped; a `last_tab_id` absent from the workspace's live tabs is cleared; a `last_tab_id` equal to `current_tab_id` is cleared (invariants 2 and 3). Repair a map holding **several** workspaces and assert stale IDs are fixed in every one of them, not only the entry a subcommand is about to touch — scoping repair to the current entry is the exact mistake [research.md](./research.md#one-repair-pass-separate-from-the-transition-rules) records three rounds of
- [x] T022 [US1] Unit-test `observe` in `src/state.rs` — a tab absent from the snapshot's tabs for that workspace is discarded and changes nothing (FR-011); a workspace with no entry gains `{ current: T, last: None }`; an entry whose `current` already equals T is left **exactly** as it was, both when it holds a target and when it holds none (FR-002); otherwise the previous current becomes `last_tab_id`
- [x] T023 [US1] Unit-test the toggle body in `src/lib.rs` against a substituted `Herdr` — one test per row of the toggle table in [data-model.md](./data-model.md#toggle--in-the-focused-workspace-w): no workspace focused; a focused workspace whose `focused_tab_id` is null or names no tab of W, asserting `observe` is **not** called; `last_tab_id` is `None` after `observe`; `last_tab_id == A`, which clears it; `tab focus` reporting `TabNotFound`, which clears the target and exits `0`; and the success row writing `{ current: target, last: A }`. Every row exits `0`
- [x] T024 [US1] Unit-test the `tab-focused` body in `src/lib.rs` — absent, unparseable, and `tab_id`/`workspace_id`-missing payloads each change nothing; a payload whose tab the snapshot does not report as focused in that workspace is discarded as stale (FR-011); otherwise `observe` runs. Then the three regression tests [data-model.md](./data-model.md#tab-focused--event-carries-w-t) owes, each starting from a `tab.closed` missed while the plugin was not running: a following `tab.focused` on a *different* tab must not promote the dead ID to `last_tab_id`; one on the *unchanged* current tab must still drop the dead target; and a still-live previous current must be promoted normally, so repair cannot swallow the ordinary path
- [x] T025 [US1] Unit-test the `tab-closed` body in `src/lib.rs` — it applies **no transition of its own**. Assert that closing the current tab leaves the previous tab as a usable target, that a stale `last_tab_id` beside a freshly replaced `current_tab_id` cannot survive, and that the payload's `tab_id` is read only to confirm the payload is well-formed with nothing branching on it
- [x] T026 [US1] Integration-test alternation and the quiet no-ops in `tests/cli.rs` — against the fake herdr, focus A then B and confirm the toggle focuses A, then B, then A (FR-003, SC-001); a workspace with no history, a closed remembered tab, and a remembered tab that is already current each exit `0` with **empty stderr** (FR-009, SC-004); a `tab focus` failing with a non-`tab_not_found` envelope exits `1` with the envelope's message on stderr; and reordering and renaming tabs between recording and invoking leaves the target unchanged (FR-004, SC-005) (depends on T020)

### Implementation for User Story 1

- [x] T027 [US1] Implement repair steps 2-4 in `src/state.rs` — `repair(&mut self, snapshot: &Snapshot)` replacing a dead `current_tab_id` with the workspace's `active_tab_id`, dropping the entry when the workspace has no live tabs, clearing a dead `last_tab_id`, and clearing a `last_tab_id` that now equals `current_tab_id`. Runs over the **whole map**, not the current entry (step 1, the workspace-level drop, is T038). Section-comment the phase split: repair drops what is dead, and only then does a transition decide whether anything happened — which is why FR-002 and FR-009 do not contradict each other (makes T021 pass)
- [x] T028 [US1] Implement `observe(&mut self, workspace, tab, snapshot)` in `src/state.rs` — the three-case rule from [data-model.md](./data-model.md#transition--at-most-one-after-repair). Comment that the middle case does double duty: it is why activating a workspace seeds nothing, and why the `tab.focused` event arriving just after a toggle is harmless (makes T022 pass, depends on T027)
- [x] T029 [US1] Implement the `toggle` body in `src/lib.rs` — take the lock, one `api snapshot`, repair, resolve W from `focused_workspace_id` falling back to `HERDR_PLUGIN_CONTEXT_JSON`'s `workspace_id` only after confirming that workspace appears in the snapshot, apply `observe(W, A, snapshot)` unless `A` is unusable, resolve the target, `tab focus` it, and on success write `{ current: target, last: A }`. Persist and release. Comment why the toggle writes its own swap rather than waiting for the hook ([research.md](./research.md#the-toggle-writes-its-own-swap)) and why no revalidation of `A` appears: W, A, and W's tab set all come from one read (makes T023 pass, depends on T028)
- [x] T030 [US1] Implement the `tab-focused` body in `src/lib.rs` — parse the event, take the lock, one snapshot, repair, discard the observation unless the snapshot currently reports T as focused in W, else `observe`; persist (makes T024 pass, depends on T028)
- [x] T031 [US1] Implement the `tab-closed` body in `src/lib.rs` — take the lock, one snapshot, repair, persist. No transition. Comment that the reduction is deliberate: the snapshot reports *every* tab that no longer exists, including ones whose close events were missed, so a hook-specific rule could only duplicate repair and drift from it (makes T025 pass, depends on T027)

**Checkpoint**: The toggle alternates between two tabs in one workspace, every no-op is silent, and
history repairs itself. US1 is independently testable — but see the note above on shipping it alone.

---

## Phase 4: User Story 2 - Each workspace remembers its own last tab (Priority: P2)

**Goal**: History is scoped per workspace: a toggle never leaves the workspace it was invoked from,
activity in one workspace never disturbs another's target, and entries for workspaces herdr no longer
reports are pruned by both paths the constitution names.

**Independent Test**: With two workspaces each holding two used tabs, toggle in workspace 1, switch to
workspace 2, toggle there, and confirm each toggle stayed inside its own workspace. Return to
workspace 1 and confirm its history was not disturbed.

### Tests for User Story 2

- [x] T032 [P] [US2] Unit-test repair step 1 in `src/state.rs` — every entry whose key is absent from the snapshot's `workspaces` is dropped, and surviving entries are untouched (FR-008). Assert the SC-008 invariant directly: after repair the number of stored entries is at most the number of workspaces the snapshot reports, and every stored key names one of them
- [x] T033 [US2] Unit-test workspace isolation in `src/lib.rs` — a toggle in W1 leaves W2's entry byte-identical (FR-007); the toggle only ever calls `focus_tab` with a tab the snapshot reports in the focused workspace (FR-005); the toggle never invokes anything that changes the focused workspace (FR-006); and a workspace with no entry is a no-op no matter how much history other workspaces hold
- [x] T034 [US2] Unit-test the workspace-activation case in `src/lib.rs` — the US2 scenario 4 sequence: W1 holds a full history, W2 has never had its tabs switched, and W2's activation re-reports its already-active tab. Assert W2 gains no history and W1's target is unchanged. This is the behavior [spec.md](./spec.md#prior-art) records prior art getting wrong, so it earns a test naming that scenario
- [x] T035 [US2] Unit-test the `workspace-closed` body in `src/lib.rs` — it applies no transition of its own, and a closed workspace's history is discarded because repair step 1 drops it (US2 scenario 5)
- [x] T036 [US2] Integration-test per-workspace scoping in `tests/cli.rs` — three workspaces with independent histories; every toggle stays inside its own workspace and disturbs no other entry (SC-003); a workspace closed **without** its close event reaching the plugin still has its entry pruned on the next invocation, which is the case FR-008 says event-based pruning cannot cover (depends on T026)
- [x] T037 [US2] Integration-test concurrency in `tests/cli.rs`, in two parts that prove different things (depends on T036):
  - **SC-009, FR-013 — no torn state.** 100 forced overlapping pairs of a `tab-focused` invocation and a `toggle`; after each pair `state.json` parses and every tab it names is live in the workspace it is filed under. This is a *durability* assertion and is satisfied by atomic replacement alone
  - **The edge case — the lock actually spans `tab focus`.** Two `toggle` processes started concurrently in the same workspace. Assert on the **sequence of `tab focus` arguments the fake herdr recorded**: the two calls must name **different** tabs, and the final `state.json` must equal the starting state. Do not assert only that the file parses and the focused tab is one of the two — those hold under precisely the interleaving this test exists to forbid, where both toggles read `{ current: A, last: B }`, both resolve `B`, both focus `B`, and both write `{ current: B, last: A }`. That state is well-formed, names a live tab, and is wrong: the user pressed twice and did not come back. [data-model.md](./data-model.md#concurrency) names this as what locking only around the read and the write would allow, so a test that cannot see it leaves the lock's whole reason unverified

  Widen the race with a **delay inside the fake herdr's `tab focus`**, not a rendezvous between the two processes. A barrier that waits for both to arrive inside the critical section deadlocks when the implementation is *correct*, because the second toggle blocks on the file lock before it ever reaches herdr — a test that hangs on success and passes on failure is worse than no test

### Implementation for User Story 2

- [x] T038 [US2] Implement repair step 1 in `src/state.rs` — drop every entry whose workspace is absent from the snapshot's `workspaces`, running before steps 2-4. Comment why invariant 1 is scoped to "whenever the plugin releases the lock": a workspace closed while the plugin is not running leaves an entry nothing can remove until the plugin next runs, and promising more would need a cleanup daemon this plugin has no business being (makes T032, T035, T036 pass)
- [x] T039 [US2] Implement the `workspace-closed` body in `src/lib.rs` — take the lock, one snapshot, repair, persist. No transition. Comment that the hook exists because Principle II names close events as a pruning path in their own right and because it keeps the file small between toggles, not because repair needs it (makes T035 pass, depends on T038)

**Checkpoint**: US1 and US2 both hold. This is the first point at which the plugin is worth
installing, and it is the PR B outcome — the feature is viable here, not after Phase 3.

---

## Phase 5: User Story 3 - Bind it to the key I want (Priority: P3)

**Goal**: A stable, documented action identifier users bind to any key of their choosing, reachable
by name before any key is bound.

**Independent Test**: Follow the README's keybinding instructions with a chosen key, reload the herdr
config, press the key, confirm the toggle runs. Change the key, reload, confirm the new key works.

### Tests for User Story 3

- [x] T040 [US3] Contract-test the manifest in `tests/cli.rs` — parse `herdr-plugin.toml` and assert `id == "quantumdancer.last-tab"`, the action's `id == "toggle"`, and that the four `command` entries name the four subcommands `src/main.rs` dispatches. The qualified `quantumdancer.last-tab.toggle` is what FR-016 makes a public contract and FR-019 protects, so a test is what stops it drifting silently in a refactor

### Implementation for User Story 3

- [x] T041 [US3] Write `README.md` — what the plugin does, the herdr version floor, and a copy-pasteable `[[keys.command]]` block using `quantumdancer.last-tab.toggle` **verbatim** (FR-017). Include install, link, list, verify, and uninstall commands, noting the asymmetry that `herdr plugin action invoke` takes the bare action id with `--plugin` while a keybinding takes the qualified form. `link` is documented as a first-class path, not an aside: it is how the plugin is developed and what US4 scenario 4 exercises
- [x] T042 [US3] Document in `README.md` that the plugin ships **no settings of its own** — the keybinding is the entire configuration surface, chosen because a plugin cannot claim a key and keybindings belong to herdr's config ([spec.md](./spec.md#assumptions)). A reader looking for a config file should find the answer rather than the absence
- [ ] T043 [US3] Follow [quickstart.md](./quickstart.md#bind-a-key) end to end against a live herdr — bind a key, reload, press it, change the chord, reload, confirm the new chord works and the old one does not, and confirm `herdr plugin action invoke toggle --plugin quantumdancer.last-tab` works before any binding exists (US3 scenarios 1-3). Principle IV requires this manual pass for the wiring seam; automated tests do not reach it

**Checkpoint**: US1, US2, and US3 hold. PR B is complete and the plugin is installable from a linked
checkout with a documented, user-chosen key.

---

## Phase 6: User Story 4 - Install without a build toolchain (Priority: P4)

**Goal**: A single install command puts a working plugin on a supported platform with no compiler
present, from an immutable release whose assets are guaranteed to exist.

**Independent Test**: On a machine with no compiler toolchain, run the documented install command and
confirm the plugin's action is registered and works.

### Tests for User Story 4

- [x] T044 [P] [US4] Test the version-to-tag mapping in `tests/cli.rs` — assert `Cargo.toml`'s `[package] version` equals `herdr-plugin.toml`'s `version`. The three copies of the number move together ([contracts/plugin-manifest.md](./contracts/plugin-manifest.md#the-version-to-tag-rule)), and the third copy — the git tag — is checked by T047 at release time because CI cannot see it
- [x] T045 [P] [US4] Test checksum verification in `tests/install.sh` — a shellcheck-clean script running `herdr/install.sh` against a local fixture, asserting it fails loudly on a mismatched sha256 rather than installing the artifact. An install script that downloads without verifying is the failure this test exists to prevent

### Implementation for User Story 4

- [x] T046 [US4] Write `herdr/install.sh` — read `version` from `herdr-plugin.toml`, construct the tag as `v<version>`, detect the platform triple, download the matching asset and its `.sha256` sidecar, verify the checksum, and install into `$HERDR_PLUGIN_ROOT/bin/herdr-last-tab`. Fail with a clear message naming the expected tag if the assets are not there — **never** fall back to `latest` or another tag, because that failure lands on users as a 404 on a release that already looks published. `set -euo pipefail`, shellcheck clean (makes T045 pass)
- [x] T047 [US4] Write `.github/workflows/release.yml`, triggered on tag push, in four jobs:
  - **Guard.** Before anything is built, fail unless the pushed tag equals `v` plus the manifest version. A mismatch is a 404 at install time on a release that already looks published, so it fails the job rather than the user
  - **Draft.** Create the release as a **draft**, with its body taken from the `CHANGELOG.md` section for this tag — `taiki-e/create-gh-release-action@v1` with `changelog: CHANGELOG.md` parses the Keep a Changelog format T049 writes and **fails when the tag has no section**, so a forgotten entry stops the release instead of shipping a bare compare link
  - **Build, attest, and exercise — one job per target, each on its own native runner.** No cross-compilation and no emulation: every target has a stock runner, so the binary is built and run on the architecture it claims to support.

    | Target | Runner |
    | --- | --- |
    | `x86_64-unknown-linux-musl` | `ubuntu-latest` |
    | `aarch64-unknown-linux-musl` | `ubuntu-24.04-arm` |
    | `x86_64-apple-darwin` | `macos-13` |
    | `aarch64-apple-darwin` | `macos-latest` |

    Each leg builds, archives with a sha256 sidecar, uploads to the draft, attaches Sigstore build provenance (`actions/attest-build-provenance`), then **extracts the archive it just uploaded and runs the binary** — `herdr-last-tab` with no subcommand, which must exit `2` with the usage message. Only the musl target needs anything extra (`rustup target add`, `musl-tools`), and that is a package install on a native host, not a foreign-architecture toolchain
  - **Publish**, gated on every leg succeeding

  The per-target *execution* is what makes FR-020 true rather than asserted, and it is where this workflow deliberately exceeds its reference. `persiyanov/herdr-reviewr` — the plugin `research.md` cites as the distribution model — builds its four targets by cross-compiling from two runner types and gates publication on all four *building*; nothing ever runs them. That was defensible when arm64 Linux runners were not freely available, and it no longer is. Building an artifact is not exercising it: a cross-compiled binary that cannot start on its own target passes a build matrix and fails the user. Extracting the uploaded archive rather than running the local build target is what makes the check cover the bytes users actually download

  If a target ever cannot be exercised, FR-020's remedy is to **drop it from `platforms` and from the release's declared set**, never to publish it unexercised
- [x] T048 [P] [US4] Write `docs/RELEASING.md` — lead with the `v<version>` mapping and the three places the number lives (`Cargo.toml`, `herdr-plugin.toml`, the tag), then the release steps, then the FR-020 rule that a platform may be declared only when **every** triple it covers was built and exercised, and the FR-021 rule for the herdr floor. Include the provenance-verification command a user or a reviewer runs against a downloaded asset — `gh attestation verify <asset> --repo QuantumDancer/herdr-last-tab` — since an attestation nobody is told how to check is decoration rather than a supply-chain control
- [x] T049 [P] [US4] Write `CHANGELOG.md` with the `0.1.0` entry, in **Keep a Changelog** format — `## [0.1.0]` headings with `### Added` / `### Compatibility` subsections, which is already the shape [contracts/plugin-manifest.md](./contracts/plugin-manifest.md#raising-the-floor-is-minor-and-carries-an-obligation) uses. The format is a machine contract here, not a style preference: T047's draft job parses this file to build the release body and fails the release when the tag has no matching section, so a heading that drifts takes the release down rather than degrading quietly. Record the **Compatibility** section that contract requires of any future release raising `min_herdr_version` — naming the old floor, the new floor, and that the previous version keeps working
- [x] T050 [US4] Extend `README.md` with `herdr plugin install QuantumDancer/herdr-last-tab`, the exact four-target set this release declares, and the note that a herdr older than the floor and a platform outside the set are both refused by **herdr** with a named error, not by the plugin (FR-020, FR-021; US4 scenarios 3 and 5)
- [ ] T051 [US4] Cut `v0.1.0` and follow [quickstart.md](./quickstart.md#installing-without-a-toolchain) — confirm T047's four smoke checks passed before the draft published, then install from the release **on each declared platform** with no compiler present, confirming on each that the action registers, the toggle works, and the reported version matches the release (US4 scenarios 1, 2, 4). T047's checks prove the binary starts on its target; this proves herdr installs and runs it there, which is the claim US4 actually makes. Where a physical machine is unavailable, a VM or container of that architecture counts and the run is **recorded** in `docs/RELEASING.md` for that release — an unrecorded run is indistinguishable from one that never happened, and FR-020 asks the release to declare only what it exercised. A platform that ends up with no recorded run is dropped from the release rather than declared on the strength of the others

**Checkpoint**: All four user stories hold. PR C is complete.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: The manual verification Principle IV requires for the seams automation cannot reach, and
the success criteria that need a live herdr.

- [ ] T052 [P] Run [quickstart.md](./quickstart.md#sc-002--under-150-ms) SC-002 on the baseline host — build the fixture first (a 10-tab workspace, history for 20 workspaces), discard the first invocation, report the **maximum** of 20 warm samples, and confirm it is under 150 ms. Record the number in `SESSION.local.md`; re-measure on a quieter machine before treating a result near the threshold as a regression
- [x] T053 [P] Run [quickstart.md](./quickstart.md#sc-004--no-op-silence) SC-004 — deliberately hit each no-op across a session of normal use and confirm zero error toasts, checking `herdr plugin log list` for a successful exit with empty stderr on each
- [x] T054 [P] Run [quickstart.md](./quickstart.md#sc-007--corrupt-state-recovers) SC-007 — corrupt, truncate, and empty `state.json` in turn; each next invocation is a clean no-op, and after focusing two distinct tabs the toggle alternates between exactly those two again
- [x] T055 [P] Run [quickstart.md](./quickstart.md#sc-008--history-never-outgrows-the-session) SC-008 — open and close 20 workspaces with some closed while the plugin is disabled, then invoke the toggle and confirm the stored entries are at most the live workspaces and every key names one of them. Check **after** the invocation, since the invariant is scoped to the window in which the plugin acts
- [x] T056 Run [quickstart.md](./quickstart.md#sc-010--writes-stay-inside-the-state-directory) SC-010 — resolve `HERDR_PLUGIN_STATE_DIR` and `HERDR_PLUGIN_CONFIG_DIR` and fix the **watch roots** from them *before* the observation starts, then watch the filesystem while toggling and churning workspaces. The observation window opens **after installation has finished** and closes **before uninstallation begins** — SC-010 scopes itself that way deliberately, and the exclusion is load-bearing rather than a convenience: `herdr/install.sh` writes the binary into `$HERDR_PLUGIN_ROOT/bin` and uninstall removes it, both outside the permitted set, so a window spanning either end would fail a correct install and make the criterion unfalsifiable. Those writes are herdr's own operations on its plugin root, not the plugin's. Two different things can go wrong here and they get opposite responses. Quickstart's pre-flight guard fires when a designated directory falls outside the subtrees the watcher covers — that is the *watch* being blind, not the plugin misbehaving, and the response is to widen the watch to cover the resolved directories and start over, because a clean result from a watcher that cannot see where a stray write would land proves nothing. A write **observed** outside the two designated directories once the watch is running is the opposite: it fails SC-010 and FR-015, and it is investigated, never accommodated by enlarging the permitted set. The permitted set is `HERDR_PLUGIN_STATE_DIR` and `HERDR_PLUGIN_CONFIG_DIR`, and nothing the run discovers may add to it
- [x] T057 Log the deferred `min_herdr_version` derivation in `SESSION.local.md` — 0.7.5 is a *verified* floor, not a known-required one, and lowering it is a testing exercise rather than a documentation edit ([research.md](./research.md#the-compatibility-floor-is-verified-not-derived)). Never committed: `SESSION.local.md` is gitignored
- [x] T058 Final gate pass across the repository — `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `shellcheck` over `herdr/*.sh` and `tests/*.sh`, with a read of every comment added along the way against the literate-style rule that comments explain the *why* rather than restate the code

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — starts immediately
- **Foundational (Phase 2)**: Depends on Setup — **blocks every user story**
- **US1 (Phase 3)**: Depends on Foundational
- **US2 (Phase 4)**: Depends on Foundational; T038's repair step 1 slots ahead of T027's steps 2-4 in
  the same `repair` function, so it lands after US1 in practice rather than beside it
- **US3 (Phase 5)**: Depends on Foundational only — the manifest exists from T004. Only **T041 and
  T042** can proceed alongside US1/US2: they write `README.md`, which nothing else touches. T040
  writes `tests/cli.rs` and so queues behind T026, T036, and T037 despite having no logical
  dependency on them; T043 needs a working toggle
- **US4 (Phase 6)**: Depends on `README.md` existing, which T041 and T042 create in US3; T050 is the
  US4 task that later extends it with the install section, not a prerequisite of its own phase.
  Otherwise independent of the behavior phases — nothing in the release machinery reads the state
  module
- **Polish (Phase 7)**: Depends on all four stories

### User Story Dependencies

- **US1 (P1)**: Independent once Foundational is done. Ships alone only as a duplicate of prior art
- **US2 (P2)**: Shares `src/state.rs` and `src/lib.rs` with US1 rather than being layered on it. It is
  independently *testable* — T032-T037 fail without it — but not independently *deliverable*, and
  [plan.md](./plan.md#delivery) puts it in the same PR for that reason
- **US3 (P3)**: Independent. The action identifier is fixed by the manifest in Phase 1
- **US4 (P4)**: Independent of US1-US3 *behavior* — nothing in it reads the state module — but not
  independent of their *files*: T044 writes `tests/cli.rs` alongside six earlier tasks, and T045
  writes `tests/install.sh`. "Packaging, CI, and docs" understates it; the test suite is in scope too

### Within Each User Story

- Tests are written and confirmed failing before the implementation task they name
- Types before the functions over them, functions before the command bodies that call them
- Unit tests against a substituted `Herdr` before integration tests against the fake herdr script
- Manual quickstart passes last, once the automated layer is green

### Parallel Opportunities

- Phase 1: T002-T006 are five different files and run together after T001
- Phase 2: T008 and T009 are different modules; T017, T018, and T019 are three different files
- Phase 3: none of T021-T025 carries `[P]`, because they fall into just two files — T021 and T022 in
  `src/state.rs`, T023-T025 in `src/lib.rs`. The *groups* are what parallelize: one person can take
  the state-module tests while another takes the command-body tests, and within each group the tasks
  serialize. The same applies to T029-T031, all three in `src/lib.rs`
- Phase 4: T032 is the only `[P]` in the phase — it is alone in `src/state.rs`, while T033-T035 share
  `src/lib.rs` with each other
- Phase 6: T048 and T049 are separate documents and run alongside the workflow work
- Phase 7: T052-T055 are independent manual scenarios and can be run in any order or by different
  people; T056 needs an install/uninstall window of its own

---

## Parallel Example: User Story 1

The unit of parallelism is the **file**, not the task, which is why no task in this phase carries
`[P]`. Two workers, one file each, working their own list in order:

```bash
# Worker A owns src/state.rs, in this order:
Task: "Unit-test the tab-level repair steps in src/state.rs"     # T021
Task: "Unit-test observe in src/state.rs"                        # T022

# Worker B owns src/lib.rs, concurrently, in this order:
Task: "Unit-test the toggle body in src/lib.rs"                  # T023
Task: "Unit-test the tab-focused body in src/lib.rs"             # T024
Task: "Unit-test the tab-closed body in src/lib.rs"              # T025
```

Note what is *not* parallel: T027-T031 all write to `src/state.rs` or `src/lib.rs`, and the repair
function in particular is touched by T027 and again by T038. Splitting those across workers produces
conflicts, not speed.

---

## Implementation Strategy

### MVP scope

**US1 + US2 together**, not US1 alone. This is the one place the template's usual "MVP is User Story
1" does not apply here, and [spec.md](./spec.md#prior-art) says why outright: US1 shipped alone
reproduces `herdr-recent-navigator`, which users already have. Per-workspace scoping is not a
follow-up — it is what makes the release worth installing.

### Incremental delivery

1. **PR A** (Phases 1-2, `chore:`) — the gates are green and `herdr plugin action list` shows
   `quantumdancer.last-tab.toggle`. Nothing works yet, and that is the point: the seams and the
   wiring are proven before any behavior depends on them
2. **PR B** (Phases 3-5, `feat:`) — the viable feature. US1, US2, and US3
3. **PR C** (Phases 6-7, `ci:`) — US4 and the manual verification, released as `v0.1.0`

Each lands through CodeRabbit and then a human; an agent never merges (Constitution, Development
Workflow).

### Parallel team strategy

The behavior phases are poor candidates for splitting: US1 and US2 share both `src/state.rs` and
`src/lib.rs`, and Phase 4 edits the very function Phase 3 wrote. The honest split is US3's
documentation and US4's release machinery, which touch no source file the behavior work touches, so
one person can carry PR C's scaffolding while another finishes PR B.

---

## Notes

- `[P]` means a different file and no dependency on an incomplete task
- Every no-op assertion checks the exit code **and** an empty stderr — a `0` with a message on stderr
  still produces the toast that FR-009 exists to prevent
- Repair is not scoped to the entry a subcommand is about to touch. Every task that touches it says
  so, because that narrowing is the defect this design has already had three rounds of
- Commit after each task or logical group, following Conventional Commits with a body explaining any
  non-obvious trade-off
- Incidental findings go in `SESSION.local.md`, never fixed opportunistically inside another change
