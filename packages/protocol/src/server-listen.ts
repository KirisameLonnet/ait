import { z } from "zod";

export const DEFAULT_DESKTOP_SERVER_LISTEN = "127.0.0.1:0";

/** Validate and format a Rust SocketAddr; port zero requests an automatic port. */
export function formatServerListen(host: string, port: string): string {
  let address = host.trim();
  if (address.startsWith("[") && address.endsWith("]")) address = address.slice(1, -1);
  if (address === "localhost") address = "127.0.0.1";
  const ipv6 = z.ipv6().safeParse(address).success;
  if (!ipv6 && !z.ipv4().safeParse(address).success) {
    throw new Error("Enter an IPv4 or IPv6 address, or localhost.");
  }
  const digits = port.trim();
  if (!/^\d{1,5}$/.test(digits) || Number(digits) > 65535) {
    throw new Error("Port must be an integer between 0 and 65535.");
  }
  const normalized = ipv6 ? new URL(`http://[${address}]`).hostname : address;
  return `${normalized}:${Number(digits)}`;
}

export function parseServerListen(listen: string): {
  host: string;
  port: string;
} {
  const match = /^(\[[^\]]+\]|[^:]+):(\d+)$/.exec(listen.trim());
  if (!match) throw new Error("Expected an IP address and port, for example 127.0.0.1:7316.");
  const normalized = formatServerListen(match[1], match[2]);
  const separator = normalized.lastIndexOf(":");
  return {
    host: normalized.slice(0, separator).replace(/^\[|\]$/g, ""),
    port: normalized.slice(separator + 1),
  };
}

export const ServerListenSchema = z.string().transform((value, ctx) => {
  try {
    const { host, port } = parseServerListen(value);
    return formatServerListen(host, port);
  } catch (error) {
    ctx.addIssue({
      code: "custom",
      message: error instanceof Error ? error.message : "Invalid listen address.",
    });
    return z.NEVER;
  }
});

/** Wildcard bind addresses are reached through the matching loopback interface. */
export function serverConnectAddress(listen: string): string {
  const { host, port } = parseServerListen(listen);
  return formatServerListen(host === "0.0.0.0" ? "127.0.0.1" : host === "::" ? "::1" : host, port);
}
