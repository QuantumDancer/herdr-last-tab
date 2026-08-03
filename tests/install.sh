#!/usr/bin/env bash
# Behavioural tests for herdr/install.sh, driven through its local-staging test seam.
#
# What these assert is narrow on purpose: an installer that downloads a binary users then execute
# on every keypress has exactly one job beyond fetching, which is to refuse. So every case below
# is a way verification can be denied its answer — a wrong digest, a missing sidecar, a missing
# archive, a sidecar that is not a digest at all, no hasher on PATH — and every one of them must
# fail loudly *and* leave an already-installed binary byte-identical. A test suite that only
# proved the happy path would pass against an installer with no verification in it.
#
# The seam is HERDR_LAST_TAB_STAGING_DIR: a local directory the assets are copied from instead of
# downloaded. Nothing here exercises curl, so the network flags in install.sh are reviewed rather
# than tested.
set -euo pipefail

REPO_ROOT="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"
INSTALL_SH="$REPO_ROOT/herdr/install.sh"
WORKFLOW="$REPO_ROOT/.github/workflows/release.yml"
BASH_BIN="$(command -v bash)"

for required in "$INSTALL_SH" "$WORKFLOW"; do
  if [ ! -f "$required" ]; then
    printf 'tests/install.sh: missing %s\n' "$required" >&2
    exit 1
  fi
done

# The fixtures have to carry the asset name for *this* host, because install.sh derives it from
# uname and there is deliberately no override for it.
os="$(uname -s)"
arch="$(uname -m)"
case "${os}-${arch}" in
  Linux-x86_64) TARGET="x86_64-unknown-linux-musl" ;;
  Linux-aarch64 | Linux-arm64) TARGET="aarch64-unknown-linux-musl" ;;
  Darwin-x86_64) TARGET="x86_64-apple-darwin" ;;
  Darwin-arm64 | Darwin-aarch64) TARGET="aarch64-apple-darwin" ;;
  *)
    printf 'tests/install.sh: unsupported host %s-%s\n' "$os" "$arch" >&2
    exit 1
    ;;
esac
ARCHIVE="herdr-last-tab-${TARGET}.tar.gz"
SIDECAR="${ARCHIVE}.sha256"

if command -v sha256sum >/dev/null 2>&1; then
  digest_of() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
  digest_of() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
  printf 'tests/install.sh: need sha256sum or shasum to build fixtures\n' >&2
  exit 1
fi

WORK="$(mktemp -d)"
trap 'rm -rf -- "$WORK"' EXIT

failures=0
CASE=""
PLUGIN=""
STAGING=""

pass() { printf 'ok   - %s\n' "$1"; }
fail() {
  printf 'FAIL - %s\n' "$1" >&2
  failures=$((failures + 1))
}

# --- Fixture construction ------------------------------------------------------------------

# A plugin checkout with the real manifest and the real script, so the version parse and the
# root-from-$0 resolution are exercised rather than stubbed.
new_case() {
  CASE="$WORK/$1"
  PLUGIN="$CASE/plugin"
  STAGING="$CASE/staging"
  mkdir -p "$PLUGIN/herdr" "$STAGING" "$CASE/home" "$CASE/build"
  cp "$REPO_ROOT/herdr-plugin.toml" "$PLUGIN/herdr-plugin.toml"
  cp "$INSTALL_SH" "$PLUGIN/herdr/install.sh"
  printf '#!/bin/sh\necho fake herdr-last-tab\n' >"$CASE/build/herdr-last-tab"
  chmod 0755 "$CASE/build/herdr-last-tab"
  tar -czf "$STAGING/$ARCHIVE" -C "$CASE/build" herdr-last-tab
}

write_sidecar() { printf '%s  %s\n' "$1" "$ARCHIVE" >"$STAGING/$SIDECAR"; }
good_sidecar() { write_sidecar "$(digest_of "$STAGING/$ARCHIVE")"; }

# A binary already at the install path, plus a pristine copy to compare against afterwards. Every
# failing case has to leave this untouched: a user whose install fails still has a working plugin.
plant_canary() {
  mkdir -p "$PLUGIN/bin"
  printf 'canary: a previously installed binary\n' >"$PLUGIN/bin/herdr-last-tab"
  chmod 0755 "$PLUGIN/bin/herdr-last-tab"
  cp "$PLUGIN/bin/herdr-last-tab" "$CASE/canary.ref"
}

# Runs the installer with the staging seam active. Extra environment assignments may be passed as
# arguments. Never lets a failure abort the suite — the exit code is the thing under test.
run_install() {
  rc=0
  env HOME="$CASE/home" HERDR_LAST_TAB_STAGING_DIR="$STAGING" "$@" \
    "$BASH_BIN" "$PLUGIN/herdr/install.sh" >"$CASE/out" 2>"$CASE/err" || rc=$?
}

# --- Assertions ----------------------------------------------------------------------------

# One shared shape for every refusal case: non-zero exit, a message that names the actual reason
# (so a case cannot pass because the script broke for an unrelated one), and an intact canary.
assert_refused() {
  local name="$1" expected="$2" ok=1
  if [ "$rc" -eq 0 ]; then
    printf '  exited 0, expected failure\n' >&2
    ok=0
  fi
  if ! grep -qF "$expected" "$CASE/err"; then
    printf '  stderr did not contain %s; got: %s\n' "$expected" "$(cat "$CASE/err")" >&2
    ok=0
  fi
  if ! cmp -s "$CASE/canary.ref" "$PLUGIN/bin/herdr-last-tab"; then
    printf '  the pre-existing binary was modified\n' >&2
    ok=0
  fi
  if [ "$ok" -eq 1 ]; then pass "$name"; else fail "$name"; fi
}

# --- Cases ---------------------------------------------------------------------------------

# The happy path is here to keep the refusal cases honest: without it they would all pass against
# a script that fails unconditionally.
new_case happy
good_sidecar
run_install
if [ "$rc" -ne 0 ]; then
  fail "verified archive installs (exit $rc: $(cat "$CASE/err"))"
elif ! cmp -s "$CASE/build/herdr-last-tab" "$PLUGIN/bin/herdr-last-tab"; then
  fail "verified archive installs (installed bytes differ from the archived binary)"
elif [ ! -x "$PLUGIN/bin/herdr-last-tab" ]; then
  fail "verified archive installs (installed binary is not executable)"
else
  pass "verified archive installs"
fi

# FR-015-adjacent, and the reason the seam is a directory rather than a URL: the installer must
# write nowhere except the plugin's own bin/ and its temp directory. This lists the whole plugin
# tree rather than sampling it, so a stray file anywhere under the root is caught.
expected_tree="$(
  printf '%s\n' \
    "$PLUGIN/bin" \
    "$PLUGIN/bin/herdr-last-tab" \
    "$PLUGIN/herdr" \
    "$PLUGIN/herdr/install.sh" \
    "$PLUGIN/herdr-plugin.toml" | LC_ALL=C sort
)"
actual_tree="$(find "$PLUGIN" -mindepth 1 | LC_ALL=C sort)"
if [ "$expected_tree" != "$actual_tree" ]; then
  fail "installer writes only into bin/ (unexpected tree: $actual_tree)"
elif [ -n "$(find "$CASE/home" -mindepth 1)" ]; then
  fail "installer writes only into bin/ (it touched \$HOME)"
else
  pass "installer writes only into bin/"
fi

new_case mismatch
plant_canary
write_sidecar "0000000000000000000000000000000000000000000000000000000000000000"
run_install
assert_refused "mismatched digest is refused" "checksum mismatch"

new_case missing-sidecar
plant_canary
rm -f "$STAGING/$SIDECAR"
run_install
assert_refused "missing sidecar is refused" "release asset $SIDECAR is missing"

new_case missing-archive
plant_canary
good_sidecar
rm -f "$STAGING/$ARCHIVE"
run_install
assert_refused "missing archive is refused" "release asset $ARCHIVE is missing"

# What a captive portal, a proxy error page, or an S3 404 body actually looks like when it lands
# where a digest was expected. The 64-hex-character shape check is what rejects it.
new_case malformed-sidecar
plant_canary
printf '<!DOCTYPE html>\n<html><head><title>404 Not Found</title></head></html>\n' >"$STAGING/$SIDECAR"
run_install
assert_refused "non-hex sidecar body is refused" "does not start with a sha256 digest"

# The failure mode this guards against is not a missing tool but the tempting response to one:
# skipping verification when no hasher is found. PATH is replaced with a directory holding every
# other command install.sh needs, so the run reaches the check and can only fail on it.
new_case no-hasher
plant_canary
good_sidecar
stub="$CASE/stub-bin"
mkdir -p "$stub"
for tool in dirname uname tr mktemp mkdir chmod cp tar mv rm; do
  tool_path="$(command -v "$tool")" || {
    printf 'tests/install.sh: %s not found; cannot build the stub PATH\n' "$tool" >&2
    exit 1
  }
  ln -s "$tool_path" "$stub/$tool"
done
run_install PATH="$stub"
assert_refused "absent sha256 utility is refused" "no sha256 utility found on PATH"

# cp follows a symlink at the destination and writes through it; mv does not. A symlink already
# sitting at the install path is therefore refused outright rather than resolved.
new_case symlink-target
mkdir -p "$PLUGIN/bin"
printf 'canary: a previously installed binary\n' >"$CASE/canary.real"
cp "$CASE/canary.real" "$CASE/canary.ref"
ln -s "$CASE/canary.real" "$PLUGIN/bin/herdr-last-tab"
good_sidecar
run_install
if [ "$rc" -eq 0 ]; then
  fail "symlink at the install path is refused (exited 0)"
elif ! grep -qF "is a symlink" "$CASE/err"; then
  fail "symlink at the install path is refused (stderr: $(cat "$CASE/err"))"
elif [ ! -L "$PLUGIN/bin/herdr-last-tab" ]; then
  fail "symlink at the install path is refused (the symlink was replaced)"
elif ! cmp -s "$CASE/canary.ref" "$CASE/canary.real"; then
  fail "symlink at the install path is refused (it was written through)"
else
  pass "symlink at the install path is refused"
fi

# --- Source-level assertions -----------------------------------------------------------------

# A fallback to a floating release pointer is the one bug that would make every other check here
# meaningless: it turns a missing asset into a silent install of some other version. Comments may
# explain why there is no fallback; code may not contain one.
if grep -qF 'releases/latest' "$INSTALL_SH"; then
  fail "install.sh has no floating-release fallback (found releases/latest)"
elif [ -n "$(grep -n 'latest' "$INSTALL_SH" | grep -Ev '^[0-9]+:[[:space:]]*#' || true)" ]; then
  fail "install.sh has no floating-release fallback (found 'latest' outside a comment)"
else
  pass "install.sh has no floating-release fallback"
fi

# The installer and the release workflow build the asset names independently, so they can drift
# apart into an install-time 404 that no build failure would catch — a failure that lands on users
# and that no build ever goes red for. This is a textual guard, and it is worth being honest about
# its reach: it checks that the workflow still declares this host's triple and still renders asset
# names from the templates below, and that the installer asked for exactly the name those
# templates produce here (the missing-archive case above captured what it asked for). It cannot
# catch a change that keeps the templates and breaks something else about the upload.
if ! grep -qE "^[[:space:]]*${TARGET}[[:space:]]" "$WORKFLOW"; then
  fail "installer and workflow agree on asset names (workflow no longer builds $TARGET)"
elif ! grep -qF 'archive: "herdr-last-tab-\(.[0]).tar.gz"' "$WORKFLOW"; then
  fail "installer and workflow agree on asset names (workflow archive template changed)"
elif ! grep -qF 'sidecar: "\(.archive).sha256"' "$WORKFLOW"; then
  fail "installer and workflow agree on asset names (workflow sidecar template changed)"
elif ! grep -qF "release asset $ARCHIVE is missing" "$WORK/missing-archive/err"; then
  fail "installer and workflow agree on asset names (installer asked for a different archive)"
else
  pass "installer and workflow agree on asset names"
fi

if [ "$failures" -ne 0 ]; then
  printf '\n%d test(s) failed\n' "$failures" >&2
  exit 1
fi
printf '\nall tests passed\n'
