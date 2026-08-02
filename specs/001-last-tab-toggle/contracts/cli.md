# Contract: the plugin binary's own CLI

**Feature**: [../spec.md](../spec.md) | **Stability**: internal — herdr invokes it, users do not

The binary is `herdr-last-tab`, invoked through `herdr/run.sh` by the manifest entries in
[plugin-manifest.md](./plugin-manifest.md). Its argument surface is not a user interface: nothing in
the README tells anyone to run it directly. It is documented here because the manifest and the tests
both depend on it, and because the exit-code contract is a constitutional requirement rather than an
implementation detail.

## Subcommands

```bash
herdr-last-tab toggle             # the user action
herdr-last-tab tab-focused        # hook for tab.focused
herdr-last-tab tab-closed         # hook for tab.closed
herdr-last-tab workspace-closed   # hook for workspace.closed
```

No flags, no options. Every input beyond the subcommand arrives through the environment. Behavior per
subcommand is the transition table in [../data-model.md](../data-model.md).

## Environment consumed

| Variable | Required | Use |
| --- | --- | --- |
| `HERDR_BIN_PATH` | yes | The herdr executable. Never a bare `herdr` from `PATH` (Principle I). |
| `HERDR_PLUGIN_STATE_DIR` | yes | Holds `state.json` and `state.lock`. The only directory written. |
| `HERDR_PLUGIN_EVENT_JSON` | hooks | The event envelope. Absent or malformed is a quiet no-op, not an error. |
| `HERDR_PLUGIN_CONTEXT_JSON` | no | A hint for the focused workspace when the snapshot reports none. Every field is optional in herdr's schema, so it is cross-checked against the snapshot before use. |

A required variable that is absent *or empty* is the FR-010 failure case. Everything else is parsed
defensively: absent, empty, and malformed are expected states (Principle I).

## Exit codes

| Exit | stderr | When |
| --- | --- | --- |
| `0` | silent | Success — and **every** FR-009 no-op: no history for this workspace, the remembered tab no longer exists, it is already the current tab, it is not in this workspace, or herdr reports nothing focused. |
| `1` | actionable message naming what was missing | `HERDR_BIN_PATH` or `HERDR_PLUGIN_STATE_DIR` missing or empty; herdr could not be run; herdr's response could not be parsed; the state directory could not be written (FR-010). |
| `2` | usage message | Unknown or missing subcommand. A wiring mistake in the manifest, never reachable from a correct install. |

The `0`-and-silent row is the one that matters most, and it is a behavioral requirement rather than a
convention. herdr surfaces a failing action as a toast. The no-op cases above are ones a user hits
routinely — pressing the key in a fresh workspace, or after closing the tab they came from — so
exiting non-zero there would train the user to ignore the toast, and with it the FR-010 failures that
are the only thing the toast is for.

The converse matters too: exiting `1` with a bare or generic message produces a toast the user cannot
act on, which is no better than silence. Each `1` names the specific thing that was missing.

## Distinguishing a dead target from an unreachable herdr

This is the split FR-009 and FR-010 forbid conflating, and it is the one prior art gets wrong. herdr
answers a failing call with a JSON envelope on stderr:

```json
{"error":{"code":"tab_not_found","message":"tab wA:t99 not found"},"id":"cli:tab:focus"}
```

Parseability is necessary but **not sufficient**. An envelope proves herdr answered; it does not
prove the answer was "that tab is gone". A protocol mismatch, a permission failure, or any code the
plugin does not recognise is herdr reporting that something is genuinely wrong, and swallowing that
as a no-op would hide precisely what FR-010 exists to surface. Classification is therefore by error
**code**, against a closed list:

| Outcome | Classification |
| --- | --- |
| Success | proceed |
| Envelope, code `tab_not_found` (from `tab focus`) | no-op — the target is gone: clear it, exit `0` |
| Envelope, **any other code** | failure — exit `1`, with the envelope's `message` on stderr |
| No envelope: spawn failure, empty output, or output that parses as neither result nor envelope | failure — exit `1` |

The code was read off a live herdr 0.7.5 rather than taken from documentation: `herdr tab focus
wA:t99` returns exactly that, with exit status 1. It is recorded in
[herdr-cli.md](./herdr-cli.md#error-envelope) alongside the command that produces it.

`api snapshot`, the plugin's only read, takes no target and so has no not-found case: any envelope
from it is a genuine failure and exits `1`.

The list is closed on purpose. An unrecognised code is treated as a failure, not as an unfamiliar
flavour of "gone", so a herdr that grows a new error the plugin has never seen makes noise rather
than silently doing nothing. Adding a code to this list is a deliberate act requiring the same
evidence the first two had: observing it from a real herdr.

## Tests this contract owes

Principle IV requires every principle to have a test that fails when it is violated. Those tests
land with the implementation, not in this specification-only change; enumerating them here is what
makes their absence visible later. At minimum:

- one test per `0`-exit no-op row, asserting both the exit code and an empty stderr;
- one per `1`-exit row, asserting the message names the missing thing;
- a `tab focus` failing with a `tab_not_found` envelope produces `0` and clears the target;
- a `tab focus` failing with a **non-target** envelope — some other code — produces `1` and puts the
  envelope's message on stderr. This is the case that distinguishes classification-by-code from
  classification-by-parseability, so without it the table above is untested;
- a spawn failure and an unparseable response each produce `1`.
