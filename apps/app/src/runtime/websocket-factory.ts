import { nativeWebSocketFactory } from "@ait/client/internal/daemon-client-websocket-transport";
import type { WebSocketFactory } from "@ait/client/internal/daemon-client-transport-types";

export function createAppWebSocketFactory(): WebSocketFactory {
  return nativeWebSocketFactory;
}
