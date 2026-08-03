#!/usr/bin/env bash
# herdr `[[build]]` step: download the prebuilt herdr-last-tab binary for this platform from the
# matching GitHub release, verify it against its sha256 sidecar, and install it into the plugin's
# own bin/ directory. This is what makes FR-018 true — installing needs no Rust toolchain.
#
# `herdr plugin link` skips [[build]] entirely, so a local checkout never runs this script; that
# path builds with `cargo build --release` and `herdr/run.sh` falls back to target/release.
#
# The binary this script installs is executed on every keypress, so the checksum comparison below
# is the whole security model: there is no build to inspect, only bytes fetched over the network.
# It is therefore unconditional and fail-closed — every failure path leaves an already-installed
# binary byte-identical rather than replacing it with something unverified.
set -euo pipefail

# Group- and world-writable bits would let another local account replace the binary between the
# install and the next keypress, so fix the mask before anything is created.
umask 022

NAME="herdr-last-tab"
REPO="QuantumDancer/herdr-last-tab"

# Tracked so the EXIT trap can remove them; the staging file is cleared once it has been renamed
# into place, because after the rename the path belongs to the installed binary.
tmp=""
stage=""

fail() {
  printf '%s: %s\n' "$NAME" "$1" >&2
  exit 1
}

cleanup() {
  if [ -n "$tmp" ]; then
    rm -rf -- "$tmp"
  fi
  if [ -n "$stage" ]; then
    rm -f -- "$stage"
  fi
}
trap cleanup EXIT

# --- Plugin root -----------------------------------------------------------------------------
#
# Resolved from this script's own location, never from $HERDR_PLUGIN_ROOT. Two independent
# reasons, both confirmed against a live herdr 0.7.5 and the reference plugin that uses the same
# mechanism: a [[build]] command may not receive the runtime environment at all (under `set -u`
# an absent variable would abort every install), and even when it is set it may name the
# post-rename runtime root while the build is executing in a staging checkout — so the binary
# would land somewhere run.sh will never look. run.sh reads $HERDR_PLUGIN_ROOT at *runtime*,
# where it is documented and correct.
ROOT="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"
case "$ROOT" in
  /*) ;;
  *) fail "could not resolve the plugin root from '$0' (got '$ROOT', which is not an absolute path)" ;;
esac
if [ ! -d "$ROOT" ]; then
  fail "resolved plugin root '$ROOT' is not a directory"
fi

MANIFEST="$ROOT/herdr-plugin.toml"
if [ ! -f "$MANIFEST" ]; then
  fail "no manifest at '$MANIFEST' — this script must run from the plugin checkout's herdr/ directory"
fi

# --- Version and tag -------------------------------------------------------------------------
#
# The tag is `v` plus the manifest version (contracts/plugin-manifest.md, "The version-to-tag
# rule"), and a mismatch surfaces as a 404 for users rather than a red build — so the version is
# parsed strictly rather than approximately. The match is anchored at the start of the line and
# required to be unique: an unanchored search also matches `min_herdr_version` and would silently
# build a download URL for herdr's floor instead of this plugin's version. The value is validated
# as semver before it is interpolated into a URL.
version_line_re='^version[[:space:]]*=[[:space:]]*"([^"]+)"'
semver_re='^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$'
version=""
version_matches=0
while IFS= read -r line || [ -n "$line" ]; do
  if [[ $line =~ $version_line_re ]]; then
    version_matches=$((version_matches + 1))
    version="${BASH_REMATCH[1]}"
  fi
done <"$MANIFEST"

if [ "$version_matches" -ne 1 ]; then
  fail "expected exactly one top-level 'version = \"...\"' in $MANIFEST, found $version_matches"
fi
if [[ ! $version =~ $semver_re ]]; then
  fail "manifest version '$version' is not a semantic version"
fi
TAG="v${version}"

# --- Platform --------------------------------------------------------------------------------
#
# An unrecognised platform fails naming the set this release declares (FR-020). It never guesses
# a nearby triple: a binary for the wrong architecture is a worse outcome than a clear refusal,
# and herdr itself refuses unsupported platforms before this script ever runs.
os="$(uname -s)"
arch="$(uname -m)"
case "${os}-${arch}" in
  # `arm64` and `aarch64` are the same architecture under two names; which one uname reports
  # varies by kernel and by userland, so both are mapped on both operating systems.
  Linux-x86_64) target="x86_64-unknown-linux-musl" ;;
  Linux-aarch64 | Linux-arm64) target="aarch64-unknown-linux-musl" ;;
  Darwin-x86_64) target="x86_64-apple-darwin" ;;
  Darwin-arm64 | Darwin-aarch64) target="aarch64-apple-darwin" ;;
  *)
    fail "no prebuilt binary for ${os}-${arch}; $TAG covers x86_64/aarch64 Linux and macOS. Build from source instead: cargo build --release, then 'herdr plugin link .'"
    ;;
esac

# Asset names. The release workflow builds the same two names from the same target triple; a
# change here without the matching change in .github/workflows/release.yml is drift that
# tests/install.sh checks for textually.
archive="${NAME}-${target}.tar.gz"
sidecar="${archive}.sha256"
base_url="https://github.com/${REPO}/releases/download/${TAG}"

# --- Checksum utility ------------------------------------------------------------------------
#
# Resolved before anything is downloaded, and its absence is fatal. There is deliberately no
# "verify if a hasher happens to exist" path: skipping verification because a tool is missing
# turns the one control this install has into an optional one, and the machines most likely to
# lack it are not the ones where unverified bytes are acceptable.
if command -v sha256sum >/dev/null 2>&1; then
  sha_tool="sha256sum"
elif command -v shasum >/dev/null 2>&1; then
  sha_tool="shasum"
else
  fail "no sha256 utility found on PATH (looked for sha256sum and shasum); refusing to install an unverified binary"
fi

sha256_of() {
  local line
  if [ "$sha_tool" = "sha256sum" ]; then
    line="$(sha256sum "$1")"
  else
    line="$(shasum -a 256 "$1")"
  fi
  # Both tools print "<digest>  <path>"; take the first field without spawning awk.
  printf '%s' "${line%% *}"
}

# --- Fetching --------------------------------------------------------------------------------
#
# HERDR_LAST_TAB_STAGING_DIR is the test seam tests/install.sh drives: a *local directory* the
# two assets are copied from. It is deliberately not a URL-base override. An override accepting
# an arbitrary https:// prefix would redirect the artifact and the digest that vouches for it to
# the same place, which defeats the only integrity control this script has; a local directory
# grants nothing to anyone who does not already control this process's environment, and
# verification below runs identically either way. It is intentionally undocumented for users.
staging="${HERDR_LAST_TAB_STAGING_DIR:-}"
if [ -n "$staging" ]; then
  case "$staging" in
    /*) ;;
    *) fail "HERDR_LAST_TAB_STAGING_DIR must be an absolute path (got '$staging')" ;;
  esac
  if [ ! -d "$staging" ]; then
    fail "HERDR_LAST_TAB_STAGING_DIR '$staging' is not a directory"
  fi
fi

# Fetch one named asset into the temp directory. Never falls back to another tag or to a
# floating release pointer: the failure a fallback would paper over is a release that is missing
# the asset it claims, and installing a different version instead is not a fix for that.
fetch_asset() {
  local asset="$1" dest="$2"

  if [ -n "$staging" ]; then
    if [ ! -f "$staging/$asset" ]; then
      fail "release asset $asset is missing for $TAG"
    fi
    cp -- "$staging/$asset" "$dest"
  else
    if ! command -v curl >/dev/null 2>&1; then
      fail "curl was not found on PATH and is required to download release assets"
    fi
    # https only, on the initial request and on every redirect, with a bounded redirect chain,
    # bounded time, and a hard failure on any HTTP error status. No --insecure, ever.
    if ! curl \
      --proto '=https' \
      --proto-redir '=https' \
      --max-redirs 5 \
      --fail \
      --location \
      --silent \
      --show-error \
      --connect-timeout 10 \
      --max-time 300 \
      --retry 3 \
      --output "$dest" \
      "${base_url}/${asset}"; then
      fail "could not download $asset from ${base_url}/${asset} — expected it on release $TAG"
    fi
  fi

  if [ ! -s "$dest" ]; then
    fail "downloaded asset $asset is empty"
  fi
}

# --- Install target ---------------------------------------------------------------------------
#
# Inspected before anything is downloaded so a hostile layout fails early, and inspected for
# symlinks specifically: writing through a symlink at either the directory or the file would let
# whoever planted it choose where the install lands. Nothing is *created* here — the directory is
# made only once the bytes have been verified, so a failed download leaves the plugin root exactly
# as it found it.
check_install_target() {
  if [ -L "$BIN_DIR" ]; then
    fail "$BIN_DIR is a symlink; refusing to install through it"
  fi
  if [ -e "$BIN_DIR" ] && [ ! -d "$BIN_DIR" ]; then
    fail "$BIN_DIR exists and is not a directory"
  fi
  if [ -L "$TARGET_PATH" ]; then
    fail "$TARGET_PATH is a symlink; refusing to install through it"
  fi
  if [ -e "$TARGET_PATH" ] && [ ! -f "$TARGET_PATH" ]; then
    fail "$TARGET_PATH exists and is not a regular file"
  fi
}

BIN_DIR="$ROOT/bin"
TARGET_PATH="$BIN_DIR/$NAME"
check_install_target

tmp="$(mktemp -d)"

printf '%s: downloading %s (%s)\n' "$NAME" "$archive" "$TAG"
fetch_asset "$archive" "$tmp/$archive"
fetch_asset "$sidecar" "$tmp/$sidecar"

# --- Verification ------------------------------------------------------------------------------
#
# Runs on the archive bytes, before extraction and before anything touches $BIN_DIR.
#
# The comparison is done by hand rather than with `sha256sum -c "$sidecar"`, which resolves the
# *filename recorded inside the sidecar* against the working directory: a file of the same name
# sitting in the process's cwd would be verified instead of the download, and the check would
# pass while the archive went unexamined.
printf '%s: verifying %s\n' "$NAME" "$archive"

recorded_line=""
IFS= read -r recorded_line <"$tmp/$sidecar" || true
recorded="${recorded_line%% *}"
recorded="$(printf '%s' "$recorded" | tr '[:upper:]' '[:lower:]')"
actual="$(sha256_of "$tmp/$archive")"
actual="$(printf '%s' "$actual" | tr '[:upper:]' '[:lower:]')"

# Shape first, equality second. Requiring 64 hex characters is what rejects a sidecar that is
# really an HTML error page, a proxy login form, or an empty file — all of which would otherwise
# reach the comparison as a garbage "expected" value.
hex_re='^[0-9a-f]{64}$'
if [ -z "$recorded" ] || [[ ! $recorded =~ $hex_re ]]; then
  fail "checksum file $sidecar does not start with a sha256 digest; refusing to install $archive"
fi
if [ -z "$actual" ] || [[ ! $actual =~ $hex_re ]]; then
  fail "could not compute a sha256 digest for $archive with $sha_tool"
fi
if [ "$recorded" != "$actual" ]; then
  fail "checksum mismatch for $archive (expected $recorded, got $actual); refusing to install"
fi

# --- Extract and install -----------------------------------------------------------------------
#
# Only the one member this plugin ships is extracted, into the temp directory: an archive that
# also carried ../../something, a device node, or a setuid file gets no opportunity to place it.
# --no-same-owner keeps ownership as the invoking user even if the archive records another.
mkdir -p "$tmp/extract"
if ! tar -xzf "$tmp/$archive" -C "$tmp/extract" --no-same-owner "$NAME"; then
  fail "archive $archive does not contain the expected member '$NAME'"
fi

extracted="$tmp/extract/$NAME"
if [ -L "$extracted" ]; then
  fail "archive member '$NAME' is a symlink; refusing to install it"
fi
if [ ! -f "$extracted" ] || [ ! -s "$extracted" ]; then
  fail "archive member '$NAME' is missing or empty"
fi

# Only now, with verified bytes in hand, is anything created under the plugin root. The layout is
# re-inspected immediately before it is written to, because the earlier check ran before the
# download: `mkdir -p` on a symlink to a directory succeeds silently, so the check has to come
# first. What remains is an unavoidable race window of a few syscalls, closed as far as it can be
# by staging inside the directory that was just checked and finishing with a rename.
check_install_target
mkdir -p "$BIN_DIR"
chmod 0755 "$BIN_DIR"

# The last step that touches the install path is a rename within the same directory, so the
# binary either is the previous one or is the fully verified new one and never a partial file.
# The staging file is created inside $BIN_DIR so the rename cannot cross a filesystem, and `mv`
# rather than `cp` because cp would follow a symlink planted at the target between the check
# above and this write.
stage="$(mktemp "$BIN_DIR/.${NAME}.XXXXXX")"
cp -- "$extracted" "$stage"
chmod 0755 "$stage"
mv -f -- "$stage" "$TARGET_PATH"
stage=""

# An install that exits 0 with nothing at the target would leave run.sh falling through to a
# development build that is not there, so the promise this script makes is checked rather than
# assumed.
if [ ! -s "$TARGET_PATH" ] || [ ! -x "$TARGET_PATH" ]; then
  fail "internal error: $TARGET_PATH is missing or not executable after install"
fi

printf '%s: installed %s (%s, sha256 %s)\n' "$NAME" "$TARGET_PATH" "$TAG" "$actual"
