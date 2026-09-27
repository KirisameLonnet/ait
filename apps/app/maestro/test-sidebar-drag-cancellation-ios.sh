#!/usr/bin/env bash
# iOS native regression harness for sidebar drag cancellation against Ait.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
FLOW_TEMPLATE="$REPO_ROOT/apps/app/maestro/sidebar-drag-cancellation-regression.yaml"
OUT_DIR="$(mktemp -d /tmp/ait-sidebar-drag-cancellation-XXXXXX)"
source "$REPO_ROOT/apps/app/maestro/support/common.sh"
FIXTURE_ROOT=""
PROJECT_IDS_FILE="$OUT_DIR/project-ids.json"

for command in git maestro node xcrun; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "Missing required command: $command" >&2
    exit 1
  fi
done

ait_maestro check

cleanup() {
  remove_rendered_flows
  if [ -s "$PROJECT_IDS_FILE" ]; then
    ait_maestro remove-projects "$PROJECT_IDS_FILE" || true
  fi
  if [ -n "$FIXTURE_ROOT" ]; then
    rm -rf "$FIXTURE_ROOT"
  fi
}
trap cleanup EXIT

if [ -z "${AIT_MAESTRO_IOS_UDID:-}" ]; then
  export AIT_MAESTRO_IOS_UDID
  AIT_MAESTRO_IOS_UDID="$(xcrun simctl list devices booted -j | node -e '
    let input = "";
    process.stdin.on("data", (chunk) => (input += chunk));
    process.stdin.on("end", () => {
      const devices = Object.values(JSON.parse(input).devices ?? {}).flat();
      const booted = devices.find((device) => device.state === "Booted");
      if (booted?.udid) process.stdout.write(booted.udid);
    });
  ')"
fi
if [ -z "$AIT_MAESTRO_IOS_UDID" ]; then
  echo "No booted iOS simulator found." >&2
  exit 1
fi

FIXTURE_ROOT="$(mktemp -d /tmp/ait-sidebar-drag-fixture-XXXXXX)"
export AIT_MAESTRO_DRAG_A_NAME="000-ait-drag-a-$(basename "$FIXTURE_ROOT")"
export AIT_MAESTRO_DRAG_B_NAME="001-ait-drag-b-$(basename "$FIXTURE_ROOT")"
export AIT_MAESTRO_DRAG_Z_NAME="zzz-ait-drag-z-$(basename "$FIXTURE_ROOT")"
PROJECT_PATHS=()
for project_name in "$AIT_MAESTRO_DRAG_A_NAME" "$AIT_MAESTRO_DRAG_B_NAME" "$AIT_MAESTRO_DRAG_Z_NAME"; do
  project_path="$FIXTURE_ROOT/$project_name"
  PROJECT_PATHS+=("$project_path")
  mkdir -p "$project_path"
  git -C "$project_path" init -b main >/dev/null
  git -C "$project_path" config user.name "Ait Maestro"
  git -C "$project_path" config user.email "maestro@ait.local"
  printf '# Sidebar drag cancellation fixture\n' > "$project_path/README.md"
  git -C "$project_path" add README.md
  git -C "$project_path" commit -m "Initial commit" >/dev/null
done

ait_maestro add-projects "$PROJECT_IDS_FILE" "${PROJECT_PATHS[@]}"
render_flow "$FLOW_TEMPLATE"
FLOW="$OUT_DIR/$(basename "$FLOW_TEMPLATE")"
echo "Running sidebar drag cancellation regression on $AIT_MAESTRO_IOS_UDID"
echo "Artifacts: $OUT_DIR"
(cd "$OUT_DIR" && maestro test "$FLOW" --udid "$AIT_MAESTRO_IOS_UDID")
