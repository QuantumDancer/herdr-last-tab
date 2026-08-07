# Changelog

All notable changes to this project are documented in this file.

The format is [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) as FR-019 requires.

The format is a machine contract here rather than a house style. The release workflow's draft job
reads the section whose heading matches the tag being released and **fails when there is none**, so
a forgotten entry stops the release instead of shipping a bare compare link — and a heading that
drifts from `## [X.Y.Z]` takes the release down rather than degrading quietly. Write the section
before you tag, and set its date to the day the release actually goes out.

<!--
Template for a release that raises `min_herdr_version`. Under contracts/plugin-manifest.md,
"Raising the floor is minor, and carries an obligation", such a release is a minor bump that MUST
carry this subsection, naming the old floor, the new floor, and the fact that the previous version
keeps working — because the user it affects gets no signal beyond an install that refuses:

### Compatibility

- Requires herdr 0.8.0 or newer (was 0.7.5). Older herdr refuses to install or link this version;
  0.4.x keeps working.

The README's stated minimum moves in the same change. A floor raise without both is incomplete.
-->

## [0.1.0] - 2026-08-07

### Added

- `quantumdancer.last-tab.toggle`: moves focus to the tab that was focused immediately before the
  current one, within the current workspace. The tmux `last-window` reflex for herdr tabs.
- Per-workspace focus history, maintained from the `tab.focused`, `tab.closed`, and
  `workspace.closed` event hooks and persisted across herdr restarts. A remembered tab that no
  longer exists, no longer belongs to the workspace that remembers it, or has become the current
  tab is dropped, and the toggle is a silent no-op rather than an error.
- Prebuilt binaries for `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`,
  `x86_64-apple-darwin`, and `aarch64-apple-darwin`, each built and exercised on a native runner.
  Installing needs no Rust toolchain (FR-018).
- Every release asset carries a `.sha256` sidecar that `herdr/install.sh` verifies before it
  installs anything, and Sigstore build provenance that anyone can verify against this repository
  and this workflow — see [docs/RELEASING.md](docs/RELEASING.md#verifying-a-downloaded-asset).
- Declared herdr floor of 0.7.5: the only version this plugin has been exercised against (FR-021).

[0.1.0]: https://github.com/QuantumDancer/herdr-last-tab/releases/tag/v0.1.0
