# Contract: the herdr surface this plugin consumes

**Feature**: [../spec.md](../spec.md) | **Verified against**: herdr 0.7.5, 2026-08-02

Principle I makes the herdr CLI the entire API. This file is the exact subset of it the plugin
depends on, captured from a live session rather than from documentation. It is the definition of
`min_herdr_version`: the floor is the oldest herdr providing everything below, and **this is the file
to re-verify when that floor moves** or when a new field is used.

Nothing outside this list may be touched — not herdr's config, session files, database, or socket.

## Commands

Both are invoked as `$HERDR_BIN_PATH <args>`. JSON is the **default** output format; there is no
`--json` flag on these commands and passing one is an error.

### `api snapshot` — the single read

One call answers every question the plugin has about live state: which workspace is focused, which
tab is active in it, which workspaces exist, and which tabs each of them holds.

```json
{"id":"cli:api:snapshot","result":{"snapshot":{
  "focused_workspace_id":"wA","focused_tab_id":"wA:t8",
  "workspaces":[{"workspace_id":"wA","active_tab_id":"wA:t8","focused":true,
                 "label":"herdr-last-tab","number":1,"pane_count":3,"tab_count":3,
                 "agent_status":"working"}],
  "tabs":[{"tab_id":"wA:t1","workspace_id":"wA","focused":false,"label":"1","number":1,
           "pane_count":1,"agent_status":"unknown"},
          {"tab_id":"wA:t8","workspace_id":"wA","focused":true,"label":"2","number":8,
           "pane_count":1,"agent_status":"working"}]},
  "protocol":17,"version":"0.7.5"}}
```

Fields used: `focused_workspace_id`; `workspaces[].workspace_id` for FR-008 reconciliation; and
`tabs[].tab_id` with `tabs[].workspace_id` for membership and target validation. The snapshot also
carries `agents`, `panes`, and `layouts`, which this plugin ignores entirely.

**Atomicity is the reason this is one call rather than two.** `workspace list` and
`tab list --workspace <id>` between them return the same facts, but as two reads with a gap: a tab
can close in that gap, and the plugin would then write an ID it had just been told was live. Because
`api snapshot` returns workspaces and tabs from one consistent view, "is this tab live in this
workspace" and "which tab is active here" can never disagree with each other. Reading more JSON than
the plugin needs is the price, and it is a good trade — it also halves the round-trips on the
latency-sensitive toggle path (SC-002).

`number` and `label` appear above only to be ruled out. They are what change when a tab is reordered
or renamed, and FR-004 forbids identifying a remembered tab by either — which is what makes SC-005
pass by construction rather than by care.

### `tab focus <tab_id>`

The only mutating call the plugin makes. It changes which tab is focused and never which workspace is
focused, which is how FR-006 holds without the plugin doing anything to enforce it.

## Error envelope

A failing call exits non-zero and writes an envelope to stderr:

```json
{"error":{"code":"tab_not_found","message":"tab wA:t99 not found"},"id":"cli:tab:focus"}
```

One code is load-bearing, and it was observed on a live 0.7.5 rather than taken from documentation:

| Command | Provoked by | Code | Exit |
| --- | --- | --- | --- |
| `tab focus wA:t99` | a tab id that does not exist, in any workspace, or a malformed id | `tab_not_found` | 1 |

That single code is the plugin's entire allowlist for "the target is gone, stay quiet". A parseable
envelope carrying **any other** code means herdr answered and the answer was not "gone", so it is a
failure the user is told about. Anything that is not an envelope at all — spawn failure, empty
output, output that parses as neither result nor envelope — means herdr could not be reached. See
[cli.md](./cli.md#distinguishing-a-dead-target-from-an-unreachable-herdr) for the full table and why
classifying on parseability alone would silence real failures.

`api snapshot` takes no arguments and describes whatever session exists, so it has no
target-not-found case at all: any envelope from it is a genuine failure. Moving to the single read
therefore shrank this allowlist rather than growing it — `tab list --workspace wZ` returns
`workspace_not_found`, and that call is no longer made.

Note that a malformed tab id yields `tab_not_found` rather than a distinct validation error, so the
plugin needs no id-shape validation of its own: a nonsense identifier read out of corrupt state is
already indistinguishable from a closed tab, and both are the same quiet no-op.

## Events

Declared in the manifest by dotted name; delivered in `HERDR_PLUGIN_EVENT_JSON` as an envelope whose
inner `type` uses underscores:

```json
{"event":"tab.focused","data":{"type":"tab_focused","tab_id":"wA:t8","workspace_id":"wA"}}
```

| Event | Required payload fields | Read by the plugin |
| --- | --- | --- |
| `tab.focused` | `type`, `tab_id`, `workspace_id` | `data.tab_id`, `data.workspace_id` |
| `tab.closed` | `type`, `tab_id`, `workspace_id` | `data.tab_id`, `data.workspace_id` |
| `workspace.closed` | `type`, `workspace_id` (optional nullable `workspace`) | `data.workspace_id` |

The plugin reads `data.<field>` and falls back to a top-level `<field>`, matching what
`herdr-last-workspace` does, so a payload delivered unwrapped still parses. An event that yields no
usable identifier is a quiet no-op.

Every event is treated as a prompt to re-read live state, never as data to act on (FR-011,
Principle III). `tab.focused` in particular is validated by confirming herdr *currently* reports that
tab as `focused` in that workspace before anything is written.

## Invocation context

`HERDR_PLUGIN_CONTEXT_JSON` deserializes herdr's `PluginInvocationContext`, whose fields are `workspace_id`,
`workspace_label`, `workspace_cwd`, `tab_id`, `tab_label`, `focused_pane_id`, `focused_pane_agent`,
`focused_pane_cwd`, `focused_pane_status`, `selected_text`, `invocation_source`, `correlation_id`,
`clicked_url`, `link_handler_id`, `worktree`.

**Every field is optional in herdr's schema.** The plugin uses only `workspace_id`, only as a fallback
when the snapshot reports no `focused_workspace_id`, and only after confirming that workspace appears
in the snapshot's `workspaces`.

## What herdr enforces so the plugin does not

Both are declared in [plugin-manifest.md](./plugin-manifest.md) and need no plugin-side code:

- **Version floor** — `plugin_requires_newer_herdr`, `plugin requires Herdr <x> or newer; current
  Herdr is <y>`. This is User Story 4 scenario 5.
- **Platform** — `platform_unsupported` outside the declared `platforms`. This is User Story 4
  scenario 3, and it is why FR-020 insists the declared set be exactly the exercised set: the message
  enumerates whatever the manifest declares, so declaration and error can never disagree.

## Directories herdr designates

Read from the environment, never constructed. Recorded here because SC-010 has to watch them:

| Variable | Value on the verification host |
| --- | --- |
| `HERDR_PLUGIN_STATE_DIR` | `~/.local/state/herdr/plugins/quantumdancer.last-tab/` |
| `HERDR_PLUGIN_CONFIG_DIR` | `~/.config/herdr/plugins/config/quantumdancer.last-tab/` |
| `HERDR_PLUGIN_ROOT` | the installed or linked checkout |

The plugin writes only under `HERDR_PLUGIN_STATE_DIR`. It ships no settings, so it never writes to the
config directory (first release, per Assumptions), and it never writes to the plugin root.
