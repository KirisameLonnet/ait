import { writeFile } from "node:fs/promises";
import path from "node:path";
import { buildHostWorkspaceRoute } from "../../src/utils/host-routes";
import { test, expect } from "../support/fixtures";
import { getServerId } from "../support/helpers/server-id";
import { seedWorkspace } from "../support/helpers/seed-client";
import {
  openChangesPanel,
  openChangesTreePanel,
  waitForWorkspaceTabsVisible,
} from "../support/helpers/workspace-tabs";

test("an oversized file shows a warning while sibling diffs remain usable", async ({ page }) => {
  const workspace = await seedWorkspace({ repoPrefix: "ait-large-diff-file-" });
  try {
    await writeFile(path.join(workspace.repoPath, "large.txt"), "a\n".repeat(150_000));
    await writeFile(path.join(workspace.repoPath, "small.txt"), "small sibling change\n");
    await page.addInitScript(() => {
      const painted = new Set<string>();
      Object.assign(window, { diffPaintedText: painted });
      const original = CanvasRenderingContext2D.prototype.fillText;
      CanvasRenderingContext2D.prototype.fillText = function (text, ...args) {
        painted.add(text);
        return original.call(this, text, ...args);
      };
    });
    await page.setViewportSize({ width: 1400, height: 900 });
    await page.goto(buildHostWorkspaceRoute(getServerId(), workspace.workspaceId));
    await waitForWorkspaceTabsVisible(page);
    await openChangesPanel(page);
    await expect(page.getByTestId("git-diff-canvas")).toBeVisible();
    await expect
      .poll(() =>
        page.evaluate(() => {
          const text = (window as unknown as { diffPaintedText: Set<string> }).diffPaintedText;
          return text.has("Diff too large to display") && text.has("small sibling change");
        }),
      )
      .toBe(true);
    await expect(page.getByTestId("diff-too-large")).toHaveCount(0);
  } finally {
    await workspace.cleanup();
  }
});

test("oversized snapshots recover without reconnecting the host", async ({ page }, testInfo) => {
  const workspace = await seedWorkspace({ repoPrefix: "ait-large-diff-snapshot-" });
  let hostConnections = 0;
  let closedConnections = 0;
  page.on("websocket", (socket) => {
    if (socket.url().includes("/v1/ws")) {
      hostConnections++;
      socket.on("close", () => closedConnections++);
    }
  });
  try {
    await writeFile(path.join(workspace.repoPath, "first.txt"), "a\n".repeat(75_000));
    await writeFile(path.join(workspace.repoPath, "second.txt"), "b\n".repeat(75_000));
    await page.setViewportSize({ width: 1400, height: 900 });
    await page.goto(buildHostWorkspaceRoute(getServerId(), workspace.workspaceId));
    await waitForWorkspaceTabsVisible(page);
    await openChangesTreePanel(page);
    let warning = page.getByTestId("changes-tree-panel").getByTestId("diff-too-large");
    await expect(warning).toBeVisible();
    await expect(warning).toContainText("This diff is too large to preview");
    const screenshot = testInfo.outputPath("oversized-diff.png");
    await page.screenshot({ path: screenshot });
    await testInfo.attach("oversized-diff", { path: screenshot, contentType: "image/png" });

    expect(hostConnections).toBeGreaterThan(0);
    const previousHostConnections = hostConnections;
    const previousClosedConnections = closedConnections;
    await writeFile(path.join(workspace.repoPath, "first.txt"), "small\n");
    await writeFile(path.join(workspace.repoPath, "second.txt"), "small\n");
    await expect(warning).toHaveCount(0);
    await openChangesPanel(page);
    await expect(page.getByTestId("git-diff-canvas")).toBeVisible();
    warning = page.getByTestId("working-diff-panel").getByTestId("diff-too-large");

    await writeFile(path.join(workspace.repoPath, "first.txt"), "a\n".repeat(1_500_000));
    await expect(warning).toBeVisible();
    await writeFile(path.join(workspace.repoPath, "first.txt"), "small again\n");
    await expect(warning).toHaveCount(0);
    await expect(page.getByTestId("git-diff-canvas")).toBeVisible();
    expect(hostConnections).toBe(previousHostConnections);
    expect(closedConnections).toBe(previousClosedConnections);
  } finally {
    await workspace.cleanup();
  }
});
