import { once } from "node:events";
import { WebSocketServer } from "ws";
import { expect, it, vi } from "vitest";
import {
  closeAllTransportSessions,
  openLocalTransportSession,
  sendLocalTransportMessage,
  type TransportEventPayload,
} from "./local-transport";

const fixture = vi.hoisted(() => ({
  events: [] as TransportEventPayload[],
  args: [] as string[][],
}));
vi.mock("electron", () => ({
  BrowserWindow: {
    getAllWindows: () => [
      {
        webContents: {
          send: (_name: string, event: TransportEventPayload) => fixture.events.push(event),
        },
      },
    ],
  },
}));
vi.mock("node:child_process", async (importOriginal) => {
  const original = await importOriginal<typeof import("node:child_process")>();
  return {
    ...original,
    spawn: (command: string, args: string[], options: object) => {
      expect(command).toBe("ssh");
      fixture.args.push(args);
      const destination = args[args.indexOf("-W") + 1];
      // A local stdio tunnel stands in for OpenSSH; no remote host or key is used.
      return original.spawn(
        process.execPath,
        [
          "-e",
          `
        const net = require('node:net');
        const socket = net.connect(Number(process.argv[1].split(':').at(-1)), '127.0.0.1');
        process.stdin.pipe(socket); socket.pipe(process.stdout);
        socket.on('error', () => process.exit(1));
      `,
          destination,
        ],
        options,
      );
    },
  };
});

it("tunnels authenticated Rust WebSockets to /v1/ws and closes the child", async () => {
  const server = new WebSocketServer({ host: "127.0.0.1", port: 0, path: "/v1/ws" });
  const token = "test-token-".repeat(4);
  let authorization: string | undefined;
  server.on("connection", (socket, request) => {
    authorization = request.headers.authorization;
    socket.on("message", (message) => socket.send(message.toString()));
  });
  try {
    await once(server, "listening");
    const address = server.address();
    if (!address || typeof address === "string") throw new Error("Missing test server port");
    openLocalTransportSession({
      sessionId: "ssh-integration",
      target: { transportType: "ssh", host: "test-host", daemonPort: address.port },
      bearerToken: token,
    });
    await vi.waitFor(() =>
      expect(fixture.events.some((event) => event.kind === "open")).toBe(true),
    );
    expect(authorization).toBe(`Bearer ${token}`);
    expect(fixture.args[0].slice(-3)).toEqual(["-W", `127.0.0.1:${address.port}`, "test-host"]);
    await sendLocalTransportMessage({ sessionId: "ssh-integration", text: '{"type":"hello"}' });
    await vi.waitFor(() =>
      expect(fixture.events).toContainEqual({
        sessionId: "ssh-integration",
        kind: "message",
        text: '{"type":"hello"}',
      }),
    );
  } finally {
    closeAllTransportSessions();
    for (const socket of server.clients) socket.terminate();
    await new Promise<void>((resolve) => server.close(() => resolve()));
  }
});
