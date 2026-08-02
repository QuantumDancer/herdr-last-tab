# Phase 1 Data Model: Per-Workspace Last Tab Toggle

**Feature**: [spec.md](./spec.md) | **Research**: [research.md](./research.md)

The plugin owns exactly one piece of durable data: a map from workspace to the two tabs that
workspace has most recently had focused. Everything else it needs, it reads back from herdr on every
invocation.

## Persisted state

Location: `$HERDR_PLUGIN_STATE_DIR/state.json`, with `$HERDR_PLUGIN_STATE_DIR/state.lock` beside it.
Both paths come from the environment; neither is ever constructed from `$HOME` (FR-015).

```rust
/// The whole of what the plugin remembers, keyed by herdr's workspace identifier.
struct PersistedState {
    workspaces: BTreeMap<String, WorkspaceHistory>,
}

/// One workspace's two-deep focus history. `last_tab_id` is the toggle target.
struct WorkspaceHistory {
    current_tab_id: String,
    last_tab_id: Option<String>,
}
```

Serialized shape:

```json
{
  "workspaces": {
    "wA": { "current_tab_id": "wA:t8", "last_tab_id": "wA:t1" },
    "wB": { "current_tab_id": "wB:t2", "last_tab_id": null }
  }
}
```

`BTreeMap` rather than `HashMap` so the file has a stable key order and two runs that make the same
change produce byte-identical output — which is what makes the SC-009 concurrency test able to
compare files rather than parse them.

**Deliberately absent**: a schema-version field, and a last-updated timestamp. A reader coming from
`herdr-last-workspace` will expect the timestamp, so its absence is a decision rather than an
oversight — nothing reads it, and the constitution puts the burden of proof on the addition. The
version field is redundant for the same reason a corrupt file is: unrecognised content and unreadable
content both degrade to "no history" under FR-014, so a version check would branch into the fallback
that already exists.

## Entities, mapped to the spec

| Spec entity | Here |
| --- | --- |
| Workspace focus history | One `BTreeMap` entry: the key is the workspace, the value is `WorkspaceHistory` |
| Remembered tab reference | `last_tab_id` together with its map key — the tab and the workspace it must belong to |
| Focus observation | Not stored. An event is a prompt to re-read live state, never trusted data (FR-011) |
| Toggle action | Not stored. `quantumdancer.last-tab.toggle`, see [contracts/plugin-manifest.md](./contracts/plugin-manifest.md) |

## Invariants

Each holds whenever the plugin releases the lock, and each traces to a requirement:

1. **Every key is a live workspace.** The repair pass drops entries herdr no longer reports, and it
   runs under the lock before anything reads or writes history (FR-008, SC-008).
2. **`last_tab_id != current_tab_id`.** A target that has collapsed onto the current tab is cleared
   rather than kept — otherwise the toggle would be a permanent no-op with no way out.
3. **A stored tab ID names a tab herdr reports in that workspace.** Enforced by validating before
   acting and repairing on failure, not by trusting what was written earlier (FR-012, SC-009).
4. **`last_tab_id: null` is not history.** An entry with no target is a workspace whose current tab
   is known but which has never been switched. It exists so the *next* switch can record a
   transition, and it makes the toggle a no-op in the meantime — which is what User Story 2
   scenarios 3 and 4 require of a workspace the user has never switched tabs in.
5. **Unreadable content is empty content.** A missing, truncated, or malformed file yields an empty
   map and is repaired by the next write. It is never an error and never a crash (FR-014).

Invariant 1 is scoped to "whenever the plugin releases the lock" on purpose. A workspace closed while
the plugin is not running leaves a stale entry that nothing can remove until the plugin next runs;
SC-008 scopes its check the same way, and promising more would require a cleanup daemon this plugin
has no business being.

## Repair, then transition

Every subcommand does the same two things under the lock before it does anything of its own: it
**repairs** stored state against the snapshot, then applies at most one **transition**. Splitting
them this way is deliberate — every stale-ID defect this design has had came from a transition rule
trying to also be a repair rule, and each fix patched one branch while the identical hole sat in the
next one.

### Repair — runs on every invocation, before anything else

Against the one snapshot, for the whole map:

1. Drop every entry whose workspace is absent from the snapshot's `workspaces` (FR-008, SC-008).
2. For each surviving entry, if `current_tab_id` is not among the snapshot's tabs for that workspace,
   replace it with that workspace's `active_tab_id`. If the workspace has no live tabs at all, drop
   the entry.
3. If `last_tab_id` is set and is not among that workspace's live tabs, clear it.
4. If `last_tab_id` now equals `current_tab_id`, clear it.

Steps 2 and 3 are what make invariant 3 true rather than aspirational, and they exist because a tab
can close while the plugin is not running: no `tab.closed` hook ever fires, so nothing else would
ever notice. Step 2 keeps a usable target where it can — closing the current tab leaves the previous
one still worth going back to — and only discards the entry when the workspace has nothing live left.
Step 4 is invariant 2.

Repair is not confined to the entry a subcommand is about to touch. A stale ID in some other
workspace is just as much a violation, it costs nothing to fix while the map is already in hand, and
scoping the pass to "the current entry" is precisely the mistake that produced three rounds of the
same finding.

This is also where FR-002 and FR-009 stop looking contradictory. FR-002 says an observation naming
the already-current tab must leave history exactly as it was; FR-009 says dropping a dead target is
required rather than a violation. Both hold because they govern different phases: repair drops what
is dead, and only then does the transition decide whether anything happened.

### Transition — at most one, after repair

> **`observe(workspace W, tab T, snapshot)`** — T is now the current tab of W.
>
> If T is not among the snapshot's tabs for W, the observation is stale: discard it and change
> nothing (FR-011).
>
> - W has no entry → create `{ current: T, last: None }`.
> - W's entry already has `current == T` → **leave it exactly as it is.**
> - Otherwise → `{ current: T, last: <the previous current> }`.

Three cases, and none of them is a repair — repair already ran, so `current` is live by the time this
rule looks at it.

The middle case is FR-002, and it does two jobs. It is why activating a workspace — which re-reports
the already-active tab — neither seeds a workspace that had no history nor overwrites the target of
one that did. And it is why the `tab.focused` event that arrives just after a toggle is harmless: the
toggle has already recorded the swap, so the event names the tab that is already current and changes
nothing. See [research.md](./research.md) for why the toggle writes its own swap rather than waiting.

Passing the whole snapshot rather than a tab list is what makes both phases trustworthy. Every
membership test in repair and in the transition resolves against one consistent view of the session,
so they cannot contradict each other the way two separate reads could.

## State transitions per subcommand

Every subcommand takes **one** `api snapshot` and does all of its reasoning against that single
consistent view. `W` is `focused_workspace_id` and `A` is the snapshot's `focused_tab_id`. Every row
runs under the lock, after repair. Every row exits `0`; the failure rows that exit `1` are in
[contracts/cli.md](./contracts/cli.md) and never reach this table, because they abort before touching
state.

`focused_workspace_id` and `focused_tab_id` are **independently nullable** in herdr's schema, and
neither is a required field — herdr does not promise they come as a pair. The first two rows of the
toggle table are that fact, not defensive padding.

### `toggle` — in the focused workspace W

After repair, the toggle applies `observe(W, A, snapshot)` and then resolves a target. Because W, A,
and W's tab set all come from one read, `A` cannot be a tab that closed between two reads: it is
either in the snapshot's tabs for W or the snapshot never claimed it was focused. That consistency is
why no revalidation of `A`, and no retry, appears below.

| Precondition | Result | Focus moves |
| --- | --- | --- |
| No workspace focused, and the invocation context names none the snapshot reports | as repair left it | no |
| A workspace is focused, but `focused_tab_id` is null or names no tab of W | as repair left it — `observe` is **not** called | no |
| After `observe`, `last_tab_id` is `None` | as `observe` left it | no |
| `last_tab_id == A` | `last_tab_id = None` | no |
| `tab focus` reports the tab does not exist | `last_tab_id = None` | no |
| `tab focus` succeeds | `{ current: <target>, last: A }` | **yes** |

The second row is the incomplete-snapshot case: a focused workspace whose focused tab herdr does not
report. Calling `observe` with an unusable `A` would write a tab id that names nothing, so the toggle
skips the observation entirely, focuses nothing, and exits `0`. Note it is *not* "change nothing at
all" — repair has already run and its results are persisted, which is the FR-009 rule that a no-op
still repairs.

There is no longer a row for "`last_tab_id` is not among the snapshot's tabs for W": repair cleared
it before this table was reached. That row's disappearance is the point of splitting repair from
transition.

The last row is FR-003's alternation: the tab just left becomes the next target, so pressing the key
again returns.

### `tab-focused` — event carries (W, T)

| Precondition | Result |
| --- | --- |
| Event JSON absent, unparseable, or missing `tab_id`/`workspace_id` | unchanged |
| The snapshot does not report T as `focused` in W | unchanged — the observation is stale (FR-011) |
| otherwise | `observe(W, T, snapshot)` |

The snapshot this hook needs in order to validate T is the same one repair used, so neither costs an
extra round-trip. Three regression tests are owed here, all of them variants of a `tab.closed` missed
while the plugin was not running: followed by a `tab.focused` on a *different* tab, which must not
promote the dead ID; followed by a `tab.focused` on the *unchanged* current tab, which must still
drop the dead target; and one confirming a still-live previous current is promoted normally, so
repair cannot swallow the ordinary path.

### `tab-closed` — event carries (W, T)

**Applies no transition of its own.** T has closed, so it is already absent from the snapshot, and
repair has therefore already done everything this hook exists to do: cleared it from `last_tab_id`,
or replaced it as `current_tab_id` with the workspace's live `active_tab_id`, or dropped the entry if
the workspace has no live tabs left. The hook's whole body is take the snapshot, repair, persist.

That is a deliberate reduction. Earlier drafts gave this hook its own rules that duplicated repair's,
and the duplicate is what let a stale `last_tab_id` survive alongside a freshly replaced
`current_tab_id` — each rule fixing the ID it was written for and neither looking at the other.

The event payload's `tab_id` is therefore read only to confirm the payload is well-formed. Nothing
branches on it, because "which tab closed" is a question the snapshot already answers better: it
reports *every* tab that no longer exists, including the ones whose close events were missed.

### `workspace-closed` — event carries W

Also applies no transition of its own, for the same reason: a closed workspace is absent from the
snapshot's `workspaces`, and repair step 1 drops it.

The hook exists because Principle II names close events as a pruning path in their own right, and
because it keeps the file small between toggles rather than letting entries accumulate until the next
invocation.

## Concurrency

The action and each event hook are separate short-lived processes with no ordering guarantee, so two
can overlap. Serialization is a property of the format and the protocol, not of whichever process
happens to run first:

1. Acquire an exclusive `fs2` lock on `state.lock` and hold it for the entire read-modify-write.
2. Read `state.json`; on any failure, proceed with an empty map.
3. Repair, apply at most one transition, serialize.
4. Write to a uniquely named temporary file in the same directory, `sync_all`, then `rename` over
   `state.json`.
5. Release the lock by dropping the guard.

`rename` within a directory is atomic, so a concurrent reader sees either the old file or the new one
and never a partial write — which is what SC-009 asserts over 100 forced overlapping pairs.

Note what the lock does **not** span: `tab focus` is issued while the lock is held, so the whole
resolve-focus-commit sequence of one toggle is serialized against any other invocation. That is what
makes two overlapping *toggles* safe, and it is a distinct pair from the one SC-009 names. SC-009
overlaps an observation with a toggle; the spec's "toggle pressed twice in quick succession" edge
case overlaps two toggles, and it needs its own integration test:

> Two `toggle` processes started concurrently in the same workspace. Afterwards the state file must
> parse, and the focused tab must be one of the two tabs that were current or target beforehand —
> never a third tab and never a closed one. Whichever toggle wins is unspecified; that the loser
> cannot interleave into a half-applied swap is not.

Serializing the herdr call rather than only the file write is a deliberate cost: it makes a toggle
wait on a concurrent hook's round-trip. The alternative — locking only around the read and again
around the write — allows two toggles to both resolve against the same target and both focus it,
which is how alternation breaks.
