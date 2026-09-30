#!/usr/bin/env bash
# Records only the Android workspace-creation repro window.
#
# The setup Maestro flow gets the app to the open sidebar with a prepared
# project visible. Recording starts after that, then the focused flow taps the
# new-workspace button, selects a provider/model, taps Create, and asserts the
# app lands on the created workspace rather than remaining on /new.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
SETUP_TEMPLATE="$REPO_ROOT/apps/app/maestro/workspace-create-android-ready-sidebar.yaml"
FOCUS_TEMPLATE="$REPO_ROOT/apps/app/maestro/workspace-create-android-create-focused.yaml"
OUT_DIR="/tmp/ait-workspace-create-android-focus-$(date +%s)"
VIDEO_DIR="/tmp/ait-maestro-videos"
DEVICE_VIDEO="/sdcard/ait-maestro-workspace-create-focused.mp4"
LOCAL_VIDEO="$VIDEO_DIR/ait-maestro-workspace-create-focused.mp4"

source "$REPO_ROOT/apps/app/maestro/support/common.sh"

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

render_flow_tree() {
  render_flow "$SETUP_TEMPLATE"
  render_flow "$FOCUS_TEMPLATE"
}

require_command adb
require_command git
require_command maestro
require_command node

mkdir -p "$OUT_DIR" "$VIDEO_DIR"
trap remove_rendered_flows EXIT

ait_maestro check

if [ -z "${AIT_MAESTRO_PROJECT_PATH:-}" ]; then
  PROJECT_PARENT="$(mktemp -d /tmp/ait-maestro-project-XXXXXX)"
  PROJECT_BASENAME="aaa-workspace-create-android-$(basename "$PROJECT_PARENT")"
  export AIT_MAESTRO_PROJECT_PATH="$PROJECT_PARENT/$PROJECT_BASENAME"
  mkdir -p "$AIT_MAESTRO_PROJECT_PATH"
  git -C "$AIT_MAESTRO_PROJECT_PATH" init >/dev/null
  git -C "$AIT_MAESTRO_PROJECT_PATH" checkout -b main >/dev/null 2>&1 || true
  git -C "$AIT_MAESTRO_PROJECT_PATH" config user.name "Ait Maestro"
  git -C "$AIT_MAESTRO_PROJECT_PATH" config user.email "maestro@ait.local"
  printf "# Workspace create Android focused recording\n" > "$AIT_MAESTRO_PROJECT_PATH/README.md"
  git -C "$AIT_MAESTRO_PROJECT_PATH" add README.md
  git -C "$AIT_MAESTRO_PROJECT_PATH" commit -m "Initial commit" >/dev/null
fi

export AIT_MAESTRO_PROJECT_NAME="${AIT_MAESTRO_PROJECT_NAME:-$(basename "$AIT_MAESTRO_PROJECT_PATH")}"

SETUP_FLOW="$OUT_DIR/workspace-create-android-ready-sidebar.yaml"
FOCUS_FLOW="$OUT_DIR/workspace-create-android-create-focused.yaml"
render_flow_tree

echo "=== Focused Android Workspace Create Recording ==="
echo "Output dir: $OUT_DIR"
echo "Video: $LOCAL_VIDEO"
echo "Project: $AIT_MAESTRO_PROJECT_PATH"
echo "Project name: $AIT_MAESTRO_PROJECT_NAME"

reverse_ait_port

echo ""
echo "Opening project in Ait..."
ait_maestro open-project "$AIT_MAESTRO_PROJECT_PATH"

echo ""
echo "Staging app at open sidebar..."
(cd "$OUT_DIR" && maestro test "$SETUP_FLOW") 2>&1 | tee "$OUT_DIR/setup.log"

echo ""
echo "Recording focused create flow..."
adb shell rm -f "$DEVICE_VIDEO" >/dev/null 2>&1 || true
adb shell screenrecord --time-limit 90 "$DEVICE_VIDEO" &
SCREENRECORD_PID=$!
sleep 1

set +e
(cd "$OUT_DIR" && maestro test "$FOCUS_FLOW") 2>&1 | tee "$OUT_DIR/focus.log"
FOCUS_STATUS=${PIPESTATUS[0]}
set -e

kill -INT "$SCREENRECORD_PID" >/dev/null 2>&1 || true
wait "$SCREENRECORD_PID" >/dev/null 2>&1 || true
adb shell pkill -INT screenrecord >/dev/null 2>&1 || true

adb pull "$DEVICE_VIDEO" "$LOCAL_VIDEO" >/dev/null
ls -lh "$LOCAL_VIDEO"

if [ "$FOCUS_STATUS" -ne 0 ]; then
  echo "Focused Maestro flow failed. Artifacts: $OUT_DIR" >&2
  exit "$FOCUS_STATUS"
fi

echo "Focused recording complete."
echo "Artifacts: $OUT_DIR"
