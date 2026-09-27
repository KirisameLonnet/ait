import type { DaemonClient } from "@ait/client/internal/daemon-client";
import { expect, test } from "../support/fixtures";
import { connectDaemonClient } from "../support/helpers/daemon-client-loader";
import { reloadPreservingHostRegistry } from "../support/helpers/hosts";
import { startIsolatedHostDaemon } from "../support/helpers/isolated-host-daemon";
import { seedSavedSettingsHosts } from "../support/helpers/settings";
import { createTempGitRepo } from "../support/helpers/workspace";

test("Ait preserves workspace identity across a process restart", async ({ page }) => {
  const server = await startIsolatedHostDaemon("workspace-restart");
  const repo = await createTempGitRepo("ait-workspace-restart-");
  let client: DaemonClient | undefined;
  try {
    client = await connectDaemonClient<DaemonClient>({
      port: server.port,
      clientIdPrefix: "restart-before",
    });
    const created = await client.createWorkspace({
      source: { kind: "directory", path: repo.path },
    });
    expect(created.error).toBeNull();
    const workspace = created.workspace!;
    await page.goto("/");
    await seedSavedSettingsHosts(page, [
      { serverId: server.serverId, endpoint: `127.0.0.1:${server.port}`, label: "Restart host" },
    ]);
    await reloadPreservingHostRegistry(page);
    const row = page.getByTestId(`sidebar-workspace-row-${server.serverId}:${workspace.id}`);
    await expect(row).toBeVisible({ timeout: 30_000 });
    await client.close();
    const previousPid = server.getPid();
    await server.restart();
    expect(server.getPid()).not.toBe(previousPid);
    client = await connectDaemonClient<DaemonClient>({
      port: server.port,
      clientIdPrefix: "restart-after",
    });
    expect((await client.fetchWorkspaces()).entries).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ id: workspace.id, projectId: workspace.projectId }),
      ]),
    );
    await reloadPreservingHostRegistry(page);
    await expect(row).toBeVisible({ timeout: 30_000 });
    await client.removeProject(workspace.projectId);
  } finally {
    await client?.close();
    await server.close();
    await repo.cleanup();
  }
});
