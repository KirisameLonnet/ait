import { describe, expect, it } from "vitest";
import { parseConnectionUri } from "@ait/protocol/daemon-endpoints";
import { readMaestroConnection } from "./ait-client";

describe("Maestro Ait connection", () => {
  it("uses the configured port and authentication for HTTP and native connections", () => {
    const config = readMaestroConnection({
      AIT_MAESTRO_SERVER_URL: "http://127.0.0.1:41234",
      AIT_MAESTRO_TOKEN: "test:token",
    });
    expect(config.infoUrl).toBe("http://127.0.0.1:41234/v1/server/info");
    expect(config.websocketUrl).toBe("ws://127.0.0.1:41234/v1/ws");
    expect(config.port).toBe(41234);
    expect(parseConnectionUri(config.connectionUri)).toMatchObject({
      host: "127.0.0.1",
      port: 41234,
      password: "test:token",
      useTls: false,
    });
  });

  it("preserves TLS and supports a separate native-device endpoint", () => {
    const config = readMaestroConnection({
      AIT_MAESTRO_SERVER_URL: "wss://[::1]/v1/ws",
      AIT_MAESTRO_TOKEN: "test-token",
      AIT_MAESTRO_DIRECT_ENDPOINT: "device.example:8443",
    });
    expect(config.infoUrl).toBe("https://[::1]/v1/server/info");
    expect(config.port).toBe(443);
    expect(parseConnectionUri(config.connectionUri)).toMatchObject({
      host: "device.example",
      port: 8443,
      useTls: true,
      password: "test-token",
    });
  });

  it("requires explicit configuration instead of falling back to a developer server", () => {
    expect(() => readMaestroConnection({})).toThrow("AIT_MAESTRO_SERVER_URL");
    expect(() =>
      readMaestroConnection({ AIT_MAESTRO_SERVER_URL: "http://localhost:41234" }),
    ).toThrow("AIT_MAESTRO_TOKEN");
  });

  it.each([
    "ws://localhost:6767/ws",
    "file:///tmp/server",
    "http://secret@localhost:1234",
    "http://localhost:1234?token=secret",
    "invalid",
  ])("rejects ambiguous or legacy URL %s without echoing credentials", (url) => {
    expect(() =>
      readMaestroConnection({ AIT_MAESTRO_SERVER_URL: url, AIT_MAESTRO_TOKEN: "secret" }),
    ).toThrow();
    try {
      readMaestroConnection({ AIT_MAESTRO_SERVER_URL: url, AIT_MAESTRO_TOKEN: "secret" });
    } catch (error) {
      expect(String(error)).not.toContain("secret");
    }
  });
});
