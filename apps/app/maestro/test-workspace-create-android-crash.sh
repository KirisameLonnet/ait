#!/usr/bin/env bash
# Android Maestro harness for the workspace-creation redirect crash.
#
# Starts from a clean app state, connects the Android app to the Ait server,
# opens a prepared git project, creates a workspace through the UI, and captures
# adb logcat around the redirect window.
#
# This harness is deliberately stronger than "composer is visible": it selects
# a model, taps Create, asserts the workspace header, asserts the New Workspace
# route is gone, and fails if logcat contains the Android Fabric view-parent
# crash signature.
#
# Usage:
#   bash apps/app/maestro/test-workspace-create-android-crash.sh
#
# Optional environment:
#   AIT_MAESTRO_APP_ID=dev.ait.mobile.debug
#   AIT_MAESTRO_SERVER_URL=http://127.0.0.1:<test-port>
#   AIT_MAESTRO_TOKEN=<test-server-token>
#   AIT_MAESTRO_PROJECT_PATH=/path/to/git/repo
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
FLOW_TEMPLATE="$REPO_ROOT/apps/app/maestro/workspace-create-android-crash.yaml"
OUT_DIR="/tmp/ait-workspace-create-android-$(date +%s)"

source "$REPO_ROOT/apps/app/maestro/support/common.sh"

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

require_command adb
require_command git
require_command maestro
require_command node

render_flow_tree() {
  render_flow "$FLOW_TEMPLATE"
}

ait_maestro check

mkdir -p "$OUT_DIR"

LOGCAT_PID=""
cleanup() {
  remove_rendered_flows
  if [ -n "$LOGCAT_PID" ]; then
    kill "$LOGCAT_PID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

if [ -z "${AIT_MAESTRO_PROJECT_PATH:-}" ]; then
  PROJECT_PARENT="$(mktemp -d /tmp/ait-maestro-project-XXXXXX)"
  PROJECT_BASENAME="aaa-workspace-create-android-$(basename "$PROJECT_PARENT")"
  export AIT_MAESTRO_PROJECT_PATH="$PROJECT_PARENT/$PROJECT_BASENAME"
  mkdir -p "$AIT_MAESTRO_PROJECT_PATH"
  git -C "$AIT_MAESTRO_PROJECT_PATH" init >/dev/null
  git -C "$AIT_MAESTRO_PROJECT_PATH" checkout -b main >/dev/null 2>&1 || true
  git -C "$AIT_MAESTRO_PROJECT_PATH" config user.name "Ait Maestro"
  git -C "$AIT_MAESTRO_PROJECT_PATH" config user.email "maestro@ait.local"
  printf "# Workspace create Android repro\n" > "$AIT_MAESTRO_PROJECT_PATH/README.md"
  git -C "$AIT_MAESTRO_PROJECT_PATH" add README.md
  git -C "$AIT_MAESTRO_PROJECT_PATH" commit -m "Initial commit" >/dev/null
else
  PROJECT_PARENT=""
fi

export AIT_MAESTRO_PROJECT_NAME="${AIT_MAESTRO_PROJECT_NAME:-$(basename "$AIT_MAESTRO_PROJECT_PATH")}"

echo "=== Workspace Create Android Crash Harness ==="
echo "Output dir: $OUT_DIR"
echo "App id: $AIT_MAESTRO_APP_ID"
echo "Project: $AIT_MAESTRO_PROJECT_PATH"
echo "Project name: $AIT_MAESTRO_PROJECT_NAME"

FLOW="$OUT_DIR/workspace-create-android-crash.yaml"
render_flow_tree
echo "Rendered flow: $FLOW"

echo ""
echo "Preparing Android port reverse..."
reverse_ait_port

echo ""
echo "Opening project in Ait..."
ait_maestro open-project "$AIT_MAESTRO_PROJECT_PATH"

echo ""
echo "Capturing Android logcat..."
adb logcat -c || true
adb logcat -v time > "$OUT_DIR/logcat.txt" &
LOGCAT_PID="$!"

echo "Running Maestro flow..."
set +e
(cd "$OUT_DIR" && maestro test "$FLOW") 2>&1 | tee "$OUT_DIR/maestro.log"
MAESTRO_STATUS=${PIPESTATUS[0]}
set -e

cleanup
LOGCAT_PID=""

if [ "$MAESTRO_STATUS" -ne 0 ]; then
  adb exec-out screencap -p > "$OUT_DIR/failure-state.png" 2>/dev/null || true
  echo ""
  echo "Maestro failed. Artifacts: $OUT_DIR" >&2
  exit "$MAESTRO_STATUS"
fi

if grep -E "failed to insert view|specified child already has a parent" "$OUT_DIR/logcat.txt" >/dev/null; then
  adb exec-out screencap -p > "$OUT_DIR/failure-state.png" 2>/dev/null || true
  echo ""
  echo "Android native view crash signature found in logcat. Artifacts: $OUT_DIR" >&2
  grep -n -E "failed to insert view|specified child already has a parent" "$OUT_DIR/logcat.txt" >&2 || true
  exit 1
fi

echo ""
echo "PASS: workspace creation flow completed without the Android view-parent crash signature."
echo "Artifacts: $OUT_DIR"
