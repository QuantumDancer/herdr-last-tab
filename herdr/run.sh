#!/usr/bin/env bash
set -euo pipefail

# herdr's two install paths put the binary in different places, and neither is
# something the manifest can express as a single literal path (see plan.md's
# Complexity Tracking entry for the full trade-off):
#
#   - `herdr plugin action install` runs the [[build]] step in herdr-plugin.toml,
#     which downloads the prebuilt binary into $HERDR_PLUGIN_ROOT/bin.
#   - `herdr plugin link .`, used for local development, skips [[build]] entirely,
#     so the only binary that exists is whatever `cargo build --release` produced
#     at $HERDR_PLUGIN_ROOT/target/release.
#
# This launcher is the seam that lets both paths resolve to a real binary: prefer
# the prebuilt install location, and fall back to the dev build when it is absent.
if [ -x "$HERDR_PLUGIN_ROOT/bin/herdr-last-tab" ]; then
  exec "$HERDR_PLUGIN_ROOT/bin/herdr-last-tab" "$@"
else
  exec "$HERDR_PLUGIN_ROOT/target/release/herdr-last-tab" "$@"
fi
