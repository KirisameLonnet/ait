import { spawn } from "node:child_process";
import { access } from "node:fs/promises";
import path from "node:path";

export const repoRoot = path.resolve(__dirname, "../../../../..");
export const reservedServerPorts = new Set([6767, 6768, 7316, 61680]);

export interface AitTestConnection {
  port: number;
  token: string;
  serverId: string;
}

// Only processes owned by this Playwright worker may be used by seed clients.
const connections = new Map<number, AitTestConnection>();

export function registerAitServer(connection: AitTestConnection): void {
  if (reservedServerPorts.has(connection.port)) throw new Error("Reserved server port");
  connections.set(connection.port, connection);
}

export function unregisterAitServer(port: number): void {
  connections.delete(port);
}

export function findAitServer(port: number): AitTestConnection | undefined {
  return connections.get(port);
}

export function requireAitServer(port: number): AitTestConnection {
  const connection = findAitServer(port);
  if (!connection) throw new Error(`Port ${port} is not an Ait server owned by this E2E worker`);
  return connection;
}

async function run(command: string, args: string[]): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    const child = spawn(command, args, { cwd: repoRoot, stdio: "inherit" });
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      if (code === 0) resolve();
      else reject(new Error(`${command} failed (${signal ?? code})`));
    });
  });
}

export async function prepareAitE2E(): Promise<void> {
  const npmCli = process.env.npm_execpath;
  if (npmCli) {
    await run(process.execPath, [npmCli, "run", "build:app-deps"]);
  } else {
    await run(process.platform === "win32" ? "npm.cmd" : "npm", ["run", "build:app-deps"]);
  }
  const configuredBinary = process.env.E2E_AIT_SERVER_BIN;
  if (!configuredBinary) {
    await run("cargo", [
      "build",
      "--target-dir",
      path.join(repoRoot, "target"),
      "-p",
      "server-bin",
      "--bin",
      "server",
    ]);
  }
  const binary = path.resolve(
    repoRoot,
    configuredBinary ??
      path.join("target", "debug", process.platform === "win32" ? "server.exe" : "server"),
  );
  await access(binary);
  process.env.E2E_AIT_SERVER_BIN = binary;
}
