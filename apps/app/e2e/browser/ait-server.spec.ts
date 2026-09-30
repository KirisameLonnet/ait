import type { DaemonClient } from "@ait/client/internal/daemon-client";
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { expect, test } from "../support/fixtures";
import { requireAitServer } from "../support/helpers/ait-server";
import { connectDaemonClient } from "../support/helpers/daemon-client-loader";
import { getE2EDaemonPort } from "../support/helpers/daemon-port";
import { seedWorkspace } from "../support/helpers/seed-client";

test("Ait requires authentication and exposes its Rust protocol", async ({ request }) => {
  const server = requireAitServer(Number(getE2EDaemonPort()));
  const origin = `http://127.0.0.1:${server.port}`;
  expect((await request.get(`${origin}/v1/server/info`)).status()).toBe(401);
  expect(
    (
      await request.get(`${origin}/v1/server/info`, {
        headers: { Authorization: `Bearer ${"x".repeat(64)}` },
      })
    ).status(),
  ).toBe(401);
  const info = await request.get(`${origin}/v1/server/info`, {
    headers: { Authorization: `Bearer ${server.token}` },
  });
  expect(info.ok()).toBe(true);
  expect(await info.json()).toMatchObject({
    server_id: server.serverId,
    protocol: { major: 1, minor: 0 },
    lifecycle: "ready",
  });
  const client = await connectDaemonClient<DaemonClient>({ clientIdPrefix: "ait-capabilities" });
  try {
    expect(client.getLastServerInfoMessage()?.features).toMatchObject({ daemonPairing: false });
    expect((await client.getDaemonStatus()).pid).toBeGreaterThan(0);
  } finally {
    await client.close();
  }
});

test("the browser connects to Ait with tickets and displays an API-created workspace", async ({
  page,
}) => {
  const workspace = await seedWorkspace({ repoPrefix: "ait-browser-" });
  const sockets: string[] = [];
  const requests: string[] = [];
  page.on("websocket", (socket) => sockets.push(socket.url()));
  page.on("request", (request) => requests.push(request.url()));
  try {
    await page.goto("/");
    const server = requireAitServer(Number(getE2EDaemonPort()));
    await expect(
      page.getByTestId(`sidebar-workspace-row-${server.serverId}:${workspace.workspaceId}`),
    ).toBeVisible({ timeout: 30_000 });
    const serverSockets = sockets.filter((url) => new URL(url).port === String(server.port));
    expect(serverSockets.length).toBeGreaterThan(0);
    expect(serverSockets.every((url) => new URL(url).pathname === "/v1/ws")).toBe(true);
    expect(requests.some((url) => url.includes("/v1/auth/ws-ticket"))).toBe(true);
    await page.goto(`/settings/hosts/${server.serverId}/host`);
    await expect(page.getByText("Online", { exact: true })).toBeVisible();
  } finally {
    await workspace.cleanup();
  }
});

test("Ait runs a native Codex turn through the offline protocol fixture", async ({ page }) => {
  const workspace = await seedWorkspace({ repoPrefix: "ait-agent-" });
  const client = await connectDaemonClient<DaemonClient>({ clientIdPrefix: "ait-agent" });
  try {
    const agent = await client.createAgent({
      provider: "codex",
      model: "offline-model",
      cwd: workspace.repoPath,
      workspaceId: workspace.workspaceId,
      title: "Offline Ait agent",
    });
    await client.sendMessage(agent.id, "Ait E2E native turn");
    const result = await client.waitForFinish(agent.id, 15_000);
    expect(result.status).not.toBe("error");
    const server = requireAitServer(Number(getE2EDaemonPort()));
    const { buildHostAgentDetailRoute } = await import("../../src/utils/host-routes");
    await page.goto(buildHostAgentDetailRoute(server.serverId, agent.id, workspace.workspaceId));
    await expect(page.getByText("Echo: Ait E2E native turn", { exact: true })).toBeVisible({
      timeout: 30_000,
    });
  } finally {
    await client.close();
    await workspace.cleanup();
  }
});

test("Ait migrates project configuration to ait.json through its API", async () => {
  const workspace = await seedWorkspace({ repoPrefix: "ait-project-config-" });
  const client = await connectDaemonClient<DaemonClient>({ clientIdPrefix: "ait-config" });
  const legacyPath = path.join(workspace.repoPath, "paseo.json");
  const original = JSON.stringify({ scripts: { old: { command: "echo legacy" } } });
  try {
    await writeFile(legacyPath, original);
    const previous = await client.readProjectConfig(workspace.repoPath);
    expect(previous.ok).toBe(true);
    if (!previous.ok) throw new Error("Failed to read legacy config");
    expect(previous.config).toEqual(JSON.parse(original));
    const config = { scripts: { current: { command: "echo ait" } } };
    const saved = await client.writeProjectConfig({
      repoRoot: workspace.repoPath,
      expectedRevision: previous.revision,
      config,
    });
    expect(saved.ok).toBe(true);
    expect(JSON.parse(await readFile(path.join(workspace.repoPath, "ait.json"), "utf8"))).toEqual(
      config,
    );
    expect(await readFile(legacyPath, "utf8")).toBe(original);
    expect(await client.readProjectConfig(workspace.repoPath)).toMatchObject({ ok: true, config });
  } finally {
    await client.close();
    await workspace.cleanup();
  }
});
