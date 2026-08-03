#!/usr/bin/env bash
set -euo pipefail

# A fixture-driven stand-in for the herdr binary. tests/cli.rs points $HERDR_BIN_PATH at this
# script so the CLI under test can be driven through a real process boundary — proving the
# spawn-and-parse path a substituted `Herdr` trait object never exercises — without a live
# herdr. Every response is scripted through environment variables so a test can produce exactly
# the JSON envelopes contracts/herdr-cli.md documents:
#
#   FAKE_HERDR_SNAPSHOT_JSON  - the `snapshot` object embedded in `api snapshot`'s success
#                                envelope. Defaults to an empty snapshot: no focused workspace,
#                                no focused tab, no workspaces, no tabs.
#   FAKE_HERDR_SNAPSHOT_ERROR - "<code>:<message>"; when set, `api snapshot` fails with this
#                                error envelope on stderr instead of succeeding.
#   FAKE_HERDR_FOCUS_RESULT   - "ok" (default) for a successful `tab focus`, or "<code>:<message>"
#                                for it to fail with that error envelope.
#   FAKE_HERDR_FOCUS_LOG      - path to append each `tab focus <tab_id>` argument to, one per
#                                line, so a test can assert on the sequence of calls a fake
#                                herdr received.
#   FAKE_HERDR_FOCUS_DELAY    - seconds to sleep before `tab focus` responds, for widening a
#                                concurrency test's race window without a rendezvous between
#                                the processes under test.

usage() {
  echo "usage: fake-herdr.sh api snapshot | tab focus <tab_id>" >&2
  exit 2
}

emit_error() {
  # $1 is "<code>:<message>"; herdr's own envelopes never contain a literal colon in the code,
  # so splitting on the first one is exact for every fixture this script is asked to produce.
  local spec="$1" id="$2"
  local code="${spec%%:*}"
  local message="${spec#*:}"
  printf '{"error":{"code":"%s","message":"%s"},"id":"%s"}\n' "$code" "$message" "$id" >&2
}

snapshot_json() {
  if [ -n "${FAKE_HERDR_SNAPSHOT_JSON:-}" ]; then
    printf '%s' "$FAKE_HERDR_SNAPSHOT_JSON"
  else
    printf '%s' '{"focused_workspace_id":null,"focused_tab_id":null,"workspaces":[],"tabs":[]}'
  fi
}

[ "$#" -ge 1 ] || usage

case "$1" in
  api)
    [ "${2:-}" = "snapshot" ] || usage
    if [ -n "${FAKE_HERDR_SNAPSHOT_ERROR:-}" ]; then
      emit_error "$FAKE_HERDR_SNAPSHOT_ERROR" "cli:api:snapshot"
      exit 1
    fi
    printf '{"id":"cli:api:snapshot","result":{"snapshot":%s}}\n' "$(snapshot_json)"
    ;;
  tab)
    [ "${2:-}" = "focus" ] || usage
    tab_id="${3:-}"
    [ -n "$tab_id" ] || usage

    if [ -n "${FAKE_HERDR_FOCUS_LOG:-}" ]; then
      echo "$tab_id" >>"$FAKE_HERDR_FOCUS_LOG"
    fi
    if [ -n "${FAKE_HERDR_FOCUS_DELAY:-}" ]; then
      sleep "$FAKE_HERDR_FOCUS_DELAY"
    fi

    result="${FAKE_HERDR_FOCUS_RESULT:-ok}"
    if [ "$result" = "ok" ]; then
      printf '{"id":"cli:tab:focus","result":{}}\n'
    else
      emit_error "$result" "cli:tab:focus"
      exit 1
    fi
    ;;
  *)
    usage
    ;;
esac
