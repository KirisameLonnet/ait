import { describe, expect, it } from "vitest";
import {
  formatServerListen,
  parseServerListen,
  ServerListenSchema,
  serverConnectAddress,
} from "./server-listen.js";

describe("server listen configuration", () => {
  it.each([
    ["localhost", "0", "127.0.0.1:0"],
    [" 0.0.0.0 ", "7316", "0.0.0.0:7316"],
    ["192.168.1.5", "65535", "192.168.1.5:65535"],
    ["::1", "080", "[::1]:80"],
    ["[0:0:0:0:0:0:0:0]", "0", "[::]:0"],
  ])("normalizes %s:%s", (host, port, expected) => {
    expect(formatServerListen(host, port)).toBe(expected);
    expect(ServerListenSchema.parse(expected)).toBe(expected);
    const fields = parseServerListen(expected);
    expect(formatServerListen(fields.host, fields.port)).toBe(expected);
  });
  it.each([
    "",
    "evil.example:7316",
    "127.0.0.1",
    "::1:7316",
    "127.0.0.1:-1",
    "127.0.0.1:65536",
    "127.0.0.1:1.5",
    "127.0.0.1:1e3",
    "ws://127.0.0.1:7316",
    "999.1.1.1:80",
    "[::1%en0]:80",
  ])('rejects "%s"', (value) => {
    expect(ServerListenSchema.safeParse(value).success).toBe(false);
  });
  it("connects wildcard listeners through loopback and preserves other IPs", () => {
    expect(serverConnectAddress("0.0.0.0:7316")).toBe("127.0.0.1:7316");
    expect(serverConnectAddress("[::]:7316")).toBe("[::1]:7316");
    expect(serverConnectAddress("192.168.1.5:7316")).toBe("192.168.1.5:7316");
  });
});
