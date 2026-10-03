#!/usr/bin/env bash

# Sourced by the native harnesses; require an explicit authenticated Ait test server.
umask 077
export AIT_MAESTRO_APP_ID="${AIT_MAESTRO_APP_ID:-dev.ait.mobile.debug}"
: "${AIT_MAESTRO_SERVER_URL:?Set the HTTP(S) origin of an Ait test server}"
: "${AIT_MAESTRO_TOKEN:?Set the Bearer token of that Ait test server}"
export AIT_MAESTRO_SERVER_URL AIT_MAESTRO_TOKEN

ait_maestro() {
  (cd "$REPO_ROOT" && node --import tsx apps/mobile/maestro/support/cli.ts "$@")
}

render_flow() {
  ait_maestro render "$1" "$OUT_DIR"
}

reverse_ait_port() {
  local port
  port="$(ait_maestro reverse-port)"
  adb reverse "tcp:$port" "tcp:$port" >/dev/null
}

# Rendered flows contain a test credential. Keep screenshots/logs, remove the flows on exit.
remove_rendered_flows() {
  if [ -d "$OUT_DIR" ]; then
    find "$OUT_DIR" -type f -name '*.yaml' -delete
  fi
}
