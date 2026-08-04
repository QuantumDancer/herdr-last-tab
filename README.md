# Herdr Last Tab

A [herdr](https://herdr.dev) plugin that toggles focus between the current tab and the previously
focused tab — the tmux `last-window` reflex, applied to herdr tabs.

The toggle is scoped **per workspace**: each workspace remembers its own last-focused pair
independently, so switching tabs in one workspace never disturbs the target another workspace
remembers. This is the plugin's reason to exist rather than a detail — a global last-tab, shared
across workspaces, gets the wrong answer as soon as you touch a second workspace, which is what
`herdr-recent-navigator` does and what this plugin fixes.

## Requirements

- herdr **0.7.5 or newer**. This is the version this plugin has been exercised against and is
  declared as `min_herdr_version` in `herdr-plugin.toml`. herdr enforces this itself — an older host
  refuses to install or link the plugin with `plugin_requires_newer_herdr` — so the plugin carries no
  code of its own to check it.

## Install

### From a release

```bash
herdr plugin install QuantumDancer/herdr-last-tab
```

This is the path a released version ships for: herdr's `install` runs the manifest's `[[build]]`
step, which downloads a prebuilt binary matching your platform and verifies its sha256 before
installing it — no Rust toolchain required.

The release declares exactly four target triples, built and exercised (not just cross-compiled) on
native runners of their own architecture:

- `x86_64-unknown-linux-musl`
- `aarch64-unknown-linux-musl`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`

Three conditions are refused, and it matters which of them **herdr itself** refuses before the
plugin ever runs and which the plugin's own install step refuses — they produce different messages,
and only the first two are herdr's:

- A herdr older than the **0.7.5** floor this plugin declares: refused by herdr with
  `plugin_requires_newer_herdr`.
- An operating system outside the two this plugin declares — anything that is neither Linux nor
  macOS: refused by herdr with `platform_unsupported`.
- A CPU architecture outside the four triples above, on an operating system that *is* declared — a
  Linux host that is neither `x86_64` nor `aarch64`, say. herdr's `platforms` key names operating
  systems, not triples, so herdr accepts this host and the refusal comes from the plugin's own
  `herdr/install.sh`, which fails with `no prebuilt binary for <os>-<arch>` and points you at
  building from source.

Uninstall an install done this way with:

```bash
herdr plugin uninstall quantumdancer.last-tab
```

### From a linked checkout

This is the development path, not a fallback for a missing release — it is how this plugin is
built and tested, and it skips the manifest's `[[build]]` step (the one that downloads a prebuilt
binary) entirely, so you need a Rust toolchain for it.

```bash
cargo build --release
herdr plugin link .
herdr plugin list
```

`herdr/run.sh` falls back to `target/release/` when it finds no downloaded binary, and that fallback
only exists once you've run `cargo build --release` yourself; if invoking the action below fails with
"no such file", that build step is what's missing. `herdr plugin list` should show the plugin as
`[local:<path>]`.

Confirm the action registered:

```bash
herdr plugin action list --plugin quantumdancer.last-tab
```

This should list `quantumdancer.last-tab` / `toggle`. You can invoke it by name before binding any
key:

```bash
herdr plugin action invoke toggle --plugin quantumdancer.last-tab
```

**Note the asymmetry**: `herdr plugin action invoke` takes the **bare** action id (`toggle`) with the
plugin named separately via `--plugin`, while a keybinding (below) uses the **qualified** form
(`quantumdancer.last-tab.toggle`). The two commands above are not interchangeable with the config
block that follows — this trips people up, so it is called out here rather than left for you to
discover.

Uninstall a linked checkout (different from the `uninstall` command above, which is for a plugin
`install`ed from a release rather than `link`ed):

```bash
herdr plugin unlink quantumdancer.last-tab
```

## Bind a key

The plugin ships no key of its own — see [Configuration](#configuration) for why. Add a
`[[keys.command]]` block to `~/.config/herdr/config.toml`, using the action identifier
`quantumdancer.last-tab.toggle` **verbatim**:

```toml
[[keys.command]]
key = "prefix+tab"
type = "plugin_action"
command = "quantumdancer.last-tab.toggle"
description = "last tab"
```

Then reload:

```bash
herdr server reload-config
```

`prefix+tab` above is only an example, chosen to echo tmux's own last-window binding. The plugin has
no opinion on which key you use — any chord your terminal delivers to herdr works. Change `key` and
reload again at any time; the old chord stops working and the new one takes over immediately.

## Configuration

This plugin ships **no settings of its own** — no config file, no plugin-specific options. The
keybinding above is the entire configuration surface.

This is deliberate, not an omission: a plugin cannot claim a key for itself, and keybindings belong
to herdr's own config, not to the plugin that the key happens to invoke. "Configurable hotkey" is
satisfied by publishing a stable, documented action identifier
(`quantumdancer.last-tab.toggle`) and letting you bind it wherever you like in herdr's config, rather
than by the plugin maintaining a key setting that would only duplicate what herdr already owns. If
you're looking for a settings file for this plugin, this is the answer: there isn't one, by design.
