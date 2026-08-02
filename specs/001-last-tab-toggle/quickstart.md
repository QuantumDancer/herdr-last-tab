# Quickstart: validating the Per-Workspace Last Tab Toggle

**Feature**: [spec.md](./spec.md) | **Contracts**: [contracts/](./contracts/)

How to get the plugin running from a local checkout and prove it does what the spec claims. Every
section below maps to a success criterion, so a full pass through this file is the manual
verification Principle IV requires for the wiring and install seams — the part automated tests cannot
reach.

`cargo test` covers the state machine and is the faster loop; run it first. This guide is for what
needs a live herdr.

## Prerequisites

- herdr **0.7.5 or newer**, running (`herdr status` reports `server: running`).
- A Rust toolchain, for the linked-checkout path. Installing a release needs none — that is the point
  of User Story 4, and it is verified separately in [Installing without a toolchain](#installing-without-a-toolchain).
- A session with at least **three workspaces**, each holding **two or more tabs**. SC-003 needs the
  three; most other scenarios need only two tabs in one of them.
- For SC-002 specifically, a heavier fixture the spec fixes rather than leaves to the tester: **one
  workspace holding 10 tabs**, and stored history for **20 workspaces**. Build it before timing
  anything — see [SC-002](#sc-002--under-150-ms).

## Install from the checkout

```bash
cargo build --release
herdr plugin link .
herdr plugin list
```

`herdr plugin link` deliberately skips the manifest's `[[build]]` step, so the prebuilt download never
runs here and `herdr/run.sh` falls back to `target/release/herdr-last-tab`. That fallback is the
reason the launcher exists; if you see "no such file", you skipped `cargo build --release`.

Confirm the action identifier is exactly what [contracts/plugin-manifest.md](./contracts/plugin-manifest.md)
promises:

```bash
herdr plugin action list --plugin quantumdancer.last-tab
```

It must list `quantumdancer.last-tab` / `toggle`. This is **User Story 3 scenario 3** — the action is
discoverable and invokable by name before any key is bound:

```bash
herdr plugin action invoke toggle --plugin quantumdancer.last-tab
```

Note the asymmetry: `invoke` takes the bare action id with `--plugin`, while a keybinding takes the
qualified form.

## Bind a key

Add to `~/.config/herdr/config.toml`, then `herdr server reload-config`:

```toml
[[keys.command]]
key = "prefix+tab"
type = "plugin_action"
command = "quantumdancer.last-tab.toggle"
description = "last tab"
```

**User Story 3 scenarios 1 and 2**: press the key and the toggle runs; change `key` to a different
chord, reload, and the new chord works while the old one does not. Any chord your terminal delivers
to herdr is fine — nothing about the plugin prefers `prefix+tab`.

## Scenarios

### SC-001 — alternation

Focus tab A, then tab B, in one workspace. Invoke the toggle 20 times, waiting for each focus change
to land before the next press. Every odd press must land on A and every even press on B, 20 for 20.

Sequences that outrun observation are deliberately excluded here; the spec's scope boundaries say
why, and the rapid case is covered by SC-009 instead.

### SC-002 — under 150 ms

Conditions are fixed by the spec: herdr no older than the declared floor, a workspace of 10 tabs,
history already stored for 20 workspaces, warm invocations only. The first invocation after install
or after a herdr restart is excluded.

Baseline host: 13th Gen Intel Core i9-13900H, 4 cores available, 8 GB RAM, Linux x86_64.

**Build the fixture first.** The numbers below are meaningless against a two-tab workspace:

1. Create a workspace and open tabs in it until `herdr tab list --workspace "$WS"` reports 10.
2. Give 20 workspaces stored history — open them and switch tabs in each, or seed
   `$HERDR_PLUGIN_STATE_DIR/state.json` directly with 20 entries naming live workspaces. Confirm with
   `jq '.workspaces | length'`.
3. Focus two distinct tabs in `$WS` so the toggle has a target, then invoke it once and discard that
   sample. That first invocation pays one-time costs the reflex case does not, and the spec excludes
   it explicitly.

**Then measure.** Each of 20 samples: record a start time, invoke the action, poll until herdr
reports the expected tab focused, record the elapsed time. Report the **maximum**, not the mean — the
criterion is about the slowest press, since one stall is what the user notices.

```bash
WS=$(herdr workspace list | jq -r '.result.workspaces[] | select(.focused) | .workspace_id')

focused_of() {
	herdr tab list --workspace "$WS" | jq -r --arg t "$1" \
		'.result.tabs[] | select(.tab_id == $t) | .focused'
}

# The two tabs the toggle will alternate between.
here=$(herdr tab list --workspace "$WS" | jq -r '.result.tabs[] | select(.focused) | .tab_id')
there=$(herdr tab list --workspace "$WS" | jq -r '.result.tabs[] | select(.focused | not) | .tab_id' | head -1)

for _ in $(seq 20); do
	start=$(date +%s%N)
	herdr plugin action invoke toggle --plugin quantumdancer.last-tab >/dev/null
	until [ "$(focused_of "$there")" = "true" ]; do :; done
	printf '%s\n' "$(( ($(date +%s%N) - start) / 1000000 ))"
	tmp=$here; here=$there; there=$tmp   # the tab just left is the next target
done | sort -n | tail -1
```

The swap at the end of the loop is the alternation itself: after a successful toggle the tab you just
left becomes the next target, so the pair trades places rather than being re-read from herdr.

Two caveats on the number this produces. The poll granularity is one `herdr tab list` process spawn,
so each sample is inflated by up to that much — the measurement is **conservative**, which is the
right direction for a threshold check but means a passing result is not a precise latency figure. And
the polling loop competes with herdr for CPU on a 4-core host; if the maximum lands near 150 ms rather
than well under it, re-measure with a quieter machine before treating it as a regression.

This cannot run in CI — it needs a live herdr with real tabs. The warm path is one read and one
write, both local socket round-trips, so the check exists to catch a regression that adds a round-trip
or a synchronous stall, not to chase a tight number.

### SC-003 — per-workspace isolation

With three workspaces each holding independent tab history, toggle in each. Every invocation must
stay inside the workspace it was invoked from, and none may disturb another workspace's target.

Then the two cases prior art gets wrong, from **User Story 2**:

- Switch tabs in W2, return to W1, toggle. W1's target must be the tab you last used *in W1* — not
  the tab you were just in over in W2.
- Activate a workspace you have never switched tabs in and toggle. Nothing happens. Activating a
  workspace re-reports its already-active tab, which records no transition, so it neither seeds that
  workspace nor disturbs W1's history.

Inspect `$HERDR_PLUGIN_STATE_DIR/state.json` between steps to confirm only the expected key changed.

### SC-004 — no-op silence

Zero error notifications across a session of normal use, including deliberately hitting each no-op:
toggling with no history, toggling after closing the remembered tab, and toggling in a workspace
where the remembered tab is already current.

herdr surfaces a failing action as a toast, so "no toast" is the observable. To see what actually
ran:

```bash
herdr plugin log list
```

Each of these must show a successful exit with empty stderr, per [contracts/cli.md](./contracts/cli.md).

### SC-005 — reorder and rename

Record a target, then reorder the tab bar and rename the target tab. Toggle. Focus must still land on
the same tab, 100% of trials. History is keyed on `tab_id`, which neither operation changes — the
`number` and `label` that do change are never read.

### SC-007 — corrupt state recovers

```bash
printf 'not json' > "$HERDR_PLUGIN_STATE_DIR/state.json"
```

The next invocation must be a clean no-op — exit 0, no toast. Then focus two **distinct** tabs in the
workspace and confirm the toggle alternates between exactly those two again. Repeat with a truncated
file and with an empty one:

```bash
truncate -s 5 "$HERDR_PLUGIN_STATE_DIR/state.json"
truncate -s 0 "$HERDR_PLUGIN_STATE_DIR/state.json"
```

### SC-008 — history never outgrows the session

Open and close 20 workspaces. Some of them must be closed **while the plugin is not running**, so no
close event ever reaches it — that is the case FR-008 says event-based pruning cannot cover, and it
is the whole point of the scenario. herdr itself has to stay up, since it is what opens and closes
workspaces; suppress the plugin rather than the server:

```bash
herdr plugin disable quantumdancer.last-tab
# open and close several workspaces here — no hook fires
herdr plugin enable quantumdancer.last-tab
```

Creating and closing those workspaces *before* linking the plugin at all works equally well.

Then invoke the toggle and read `state.json`: the number of stored entries must be at most the number
of workspaces `herdr workspace list` reports, and every stored key must be one of them.

Check after the invocation, not before. The invariant is scoped to the window in which the plugin
acts — a workspace closed while the plugin was disabled leaves an entry until the next run, and that
is by design.

### SC-009 — concurrent writes never corrupt

Force 100 overlapping pairs of a focus observation and a toggle invocation. After every pair,
`state.json` must parse, and every tab it names must be one herdr reports live in the workspace it is
filed under.

The write protocol that makes this hold — one exclusive lock across the whole read-modify-write, then
temp-file-and-rename — is in [data-model.md](./data-model.md#concurrency). This scenario is what makes
FR-013 falsifiable rather than aspirational.

### SC-010 — writes stay inside the state directory

Watch the filesystem for the whole window between the end of installation and the start of
uninstallation, while toggling, switching tabs, and opening and closing workspaces.

The watch roots have to be derived from the directories herdr actually handed the plugin, not
assumed. `HERDR_PLUGIN_STATE_DIR` and `HERDR_PLUGIN_CONFIG_DIR` come from herdr's environment and can
sit outside `$HOME` — a system install, a relocated `XDG_STATE_HOME`, a container. Watching `/home`
and `/tmp` while the state directory lives in `/var/lib` would produce a clean result that proves
nothing, which is worse than a failure:

```bash
state=$(realpath "$HERDR_PLUGIN_STATE_DIR")
config=$(realpath "$HERDR_PLUGIN_CONFIG_DIR")

# Fail loudly rather than silently watching the wrong subtree.
for dir in "$state" "$config"; do
	case "$dir" in
		"$HOME"/*|/tmp/*) ;;
		*) printf 'SC-010 cannot be checked: %s is outside the watched roots\n' "$dir" >&2; exit 1 ;;
	esac
done

# Linux
inotifywait -m -r --format '%w%f %e' "$HOME" /tmp
```

If the guard trips, widen the roots to cover the resolved directories and re-run — do not narrow the
claim. The point of the criterion is that *no* write lands outside those two directories, so a watch
that cannot see where a stray write would go cannot falsify it.

No file outside `HERDR_PLUGIN_STATE_DIR` and `HERDR_PLUGIN_CONFIG_DIR` may be created or modified by
the plugin. Install and uninstall are excluded on purpose: those are herdr's own operations on its
plugin root and registry, so writes there are not the plugin's doing.

## Installing without a toolchain

**User Story 4**, and the only part of this guide that needs a release to exist:

```bash
herdr plugin install QuantumDancer/herdr-last-tab
```

On a supported platform with no compiler present, this must install and register the action —
`herdr plugin action list` shows it, and the reported version matches the release. On a platform
outside the declared set, herdr refuses with `platform_unsupported` and names the supported targets.
On a herdr older than the declared floor, herdr refuses with `plugin_requires_newer_herdr` and names
the requirement. Both refusals are herdr's, not the plugin's.

## Uninstall

```bash
herdr plugin unlink quantumdancer.last-tab      # for a linked checkout
herdr plugin uninstall quantumdancer.last-tab   # for an installed release
```

Both take the plugin id, not a path — `link` is the only one of the four that takes a directory.
