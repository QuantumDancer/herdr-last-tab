# Contract: `herdr-plugin.toml`

**Feature**: [../spec.md](../spec.md) | **Stability**: public — see the compatibility rules below

This is the plugin's public interface. herdr reads it, users bind keys against what it declares, and
FR-019 makes changing it a versioning event. It is a contract rather than a configuration file.

## The manifest

```toml
id = "quantumdancer.last-tab"
name = "Last Tab"
version = "0.1.0"
# Verified floor, not a derived one: 0.7.5 is the only herdr this plugin has been exercised
# against. Every event and field it uses exists at 0.7.5; whether they exist earlier is untested.
# See research.md and the deferred task in SESSION.local.md.
min_herdr_version = "0.7.5"
description = "Toggle focus between the current tab and the previously focused tab, per workspace."
platforms = ["linux", "macos"]

# Downloads the prebuilt binary for this platform into $HERDR_PLUGIN_ROOT/bin, so installing
# needs no Rust toolchain (FR-018). `herdr plugin link` skips this step, which is why every
# entry point below goes through run.sh rather than naming a path directly.
[[build]]
command = ["bash", "herdr/install.sh"]
platforms = ["linux", "macos"]

[[actions]]
id = "toggle"
title = "Last tab"
contexts = ["global", "workspace", "tab"]
command = ["bash", "herdr/run.sh", "toggle"]

[[events]]
on = "tab.focused"
command = ["bash", "herdr/run.sh", "tab-focused"]

[[events]]
on = "tab.closed"
command = ["bash", "herdr/run.sh", "tab-closed"]

[[events]]
on = "workspace.closed"
command = ["bash", "herdr/run.sh", "workspace-closed"]
```

## What each field promises

| Field | Promise |
| --- | --- |
| `id` | `quantumdancer.last-tab`. Combined with the action id, this is what users bind. |
| `version` | Strict semver (FR-019), and bound to the git tag by the rule below. |
| `min_herdr_version` | The oldest herdr the release was verified against (FR-021). herdr enforces it and rejects an older host itself, with `plugin_requires_newer_herdr`. |
| `platforms` | Exactly the targets the release was exercised on (FR-020). herdr raises `platform_unsupported` elsewhere. |
| `contexts` | `global` so the action is reachable from anywhere, `workspace` and `tab` because it is meaningful from both. Validated against herdr's five-variant enum. |

## The version-to-tag rule

For a manifest `version` of `X.Y.Z`, the release tag is **`vX.Y.Z`** — the version with a single
leading `v`, and nothing else. The first release is `version = "0.1.0"` with tag `v0.1.0`.

They are not equal strings, and writing the rule as "the version must equal the tag" was wrong. The
coupling is a mapping, and it is load-bearing: `herdr/install.sh` derives the download URL from the
manifest version, so `https://github.com/QuantumDancer/herdr-last-tab/releases/download/v0.1.0/…`
only resolves if the tag applies that exact transformation. A mismatch is not a build failure — it is
a 404 at install time on a release that already looks published.

Because the failure lands on users rather than on CI, both ends validate it rather than trusting the
release checklist:

- `herdr/install.sh` reads `version` from the manifest, constructs `v<version>`, and fails with a
  clear message if the release assets are not there — never falling back to `latest` or to another
  tag.
- The release workflow, which triggers on the tag, checks that the pushed tag equals `v` plus the
  manifest version and fails the job before building if it does not.

`Cargo.toml`'s `[package] version` is a third copy of the same number and moves with the other two.
The single rule stated here is the one `plan.md` and `research.md` refer to; none of them restates
it, so there is one place to change if it ever moves.

## The action identifier

```text
quantumdancer.last-tab.toggle
```

Qualified as `<plugin id>.<action id>`. This exact string is the contract FR-016 names: it is what
`herdr plugin action list` shows, what the README's keybinding example uses verbatim, and what
FR-019 protects. The keybinding users write:

```toml
[[keys.command]]
key = "prefix+tab"
type = "plugin_action"
command = "quantumdancer.last-tab.toggle"
description = "last tab"
```

The plugin requires no particular key and ships no key of its own. `prefix+tab` above is an example
chosen to echo tmux; any chord the user's terminal delivers to herdr works.

Note that `herdr plugin action invoke` takes the **bare** action id with the plugin named separately —
`herdr plugin action invoke toggle --plugin quantumdancer.last-tab` — while a keybinding takes the
qualified form. Both appear in the README because the first is how User Story 3 scenario 3 verifies
the action before binding it.

## Compatibility rules

Under FR-019, applied to this file:

- **Major** — renaming `id` or the action's `id`, changing what the toggle observably does in a way
  that breaks an existing setup, or removing a declared platform. A rename also requires a
  `BREAKING CHANGE:` footer and a README migration note in the same change.
- **Minor** — adding a platform to `platforms`, adding an action or event hook, raising
  `min_herdr_version` (with the obligation below).
- **Patch** — everything else, including `description` and `title`. Lowering `min_herdr_version` is a
  patch: a floor that turns out to be too conservative is not a breaking change to relax.

### Raising the floor is minor, and carries an obligation

A raised `min_herdr_version` is a **minor** bump, because it leaves working setups working. herdr
refuses the *upgrade* on an older host; it does not disturb the version already installed there.
That is FR-019's test for major — "would break an existing user's setup" — and this does not meet it.
The floor also only ever rises alongside adopting a herdr capability the plugin newly depends on,
which FR-019 independently classes as minor, so the alternative would make every such adoption a
major release.

What is real is the quieter risk: a user on an older herdr stops receiving updates and gets no signal
beyond an install that refuses. So the classification comes with an obligation rather than a bigger
number. A release that raises the floor MUST carry a **Compatibility** entry in `CHANGELOG.md` naming
the old floor, the new floor, and the fact that the previous version keeps working:

```markdown
## [0.5.0]

### Compatibility

- Requires herdr 0.8.0 or newer (was 0.7.5). Older herdr will refuse to install or link this
  version; 0.4.x keeps working.
```

The README's stated minimum is updated in the same change. A floor raise without both is incomplete,
in the same way that a renamed action id without a migration note is incomplete.

The floor is recorded with the change that needs it, never speculatively — the constitution requires
the dependency and the floor to move together.
