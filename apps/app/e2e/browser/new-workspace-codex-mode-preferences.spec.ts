import type { FormPreferences } from "@/create-agent-preferences/preferences";
import type { DaemonClient } from "@getpaseo/client/internal/daemon-client";
import { test } from "../support/creation-fixtures";
import { expect, type Page } from "../support/fixtures";
import { connectDaemonClient } from "../support/helpers/daemon-client-loader";
import { openAgentRoute } from "../support/helpers/mock-agent";
import {
  openGlobalNewWorkspaceComposer,
  selectNewWorkspaceProject,
} from "../support/helpers/new-workspace";
import { expectNoTruncation } from "../support/helpers/no-truncation";
import { escapeRegex } from "../support/helpers/regex";
import { seedWorkspace } from "../support/helpers/seed-client";

const CREATE_AGENT_PREFERENCES_KEY = "@paseo:create-agent-preferences";

async function seedCodexDefaultPermissionPreferences(page: Page, cwd: string): Promise<string> {
  const client = await connectDaemonClient<DaemonClient>({
    clientIdPrefix: "codex-mode-preferences",
  });
  try {
    await expect
      .poll(
        async () =>
          (await client.getProvidersSnapshot({ cwd })).entries.find(
            (entry) => entry.provider === "codex",
          )?.status,
      )
      .toBe("ready");
    const snapshot = await client.getProvidersSnapshot({ cwd });
    const model = snapshot.entries
      .find((entry) => entry.provider === "codex")
      ?.models?.find((candidate) => candidate.thinkingOptions?.length);
    if (!model?.thinkingOptions?.[0])
      throw new Error("Codex catalogue must expose a model with thinking options");
    await page.addInitScript(
      ({ preferencesKey, modelId, thinkingOptionId }) => {
        localStorage.setItem(
          preferencesKey,
          JSON.stringify({
            provider: "codex",
            providerPreferences: {
              codex: {
                model: modelId,
                mode: "auto",
                thinkingByModel: { [modelId]: thinkingOptionId },
              },
              mock: { model: "ten-second-stream" },
            },
          } satisfies FormPreferences),
        );
      },
      {
        preferencesKey: CREATE_AGENT_PREFERENCES_KEY,
        modelId: model.id,
        thinkingOptionId: model.thinkingOptions[0].id,
      },
    );
    return model.id;
  } finally {
    await client.close();
  }
}

async function selectMode(page: Page, label: string): Promise<void> {
  const modeControl = page.getByRole("button", { name: /^Select agent mode \(/ });
  await expect(modeControl).toBeVisible({ timeout: 30_000 });
  await modeControl.click();

  const popup = page.getByTestId("combobox-desktop-container").last();
  await expect(popup).toBeVisible({ timeout: 10_000 });
  await expectNoTruncation(popup);

  const searchInput = page.getByRole("textbox", { name: /search mode/i });
  await expect(searchInput).toBeVisible({ timeout: 10_000 });
  await searchInput.fill(label);

  const option = popup.getByText(new RegExp(`^${escapeRegex(label)}$`, "i")).first();
  await expect(option).toBeVisible({ timeout: 10_000 });
  await option.click({ force: true });
  await expect(searchInput).not.toBeVisible({ timeout: 5_000 });
}

test.describe("New workspace Codex mode preferences", () => {
  test.describe.configure({ timeout: 240_000 });

  test("uses the live Codex agent mode as the next New Workspace default", async ({ page }) => {
    const seeded = await seedWorkspace({ repoPrefix: "codex-live-mode-preferences-" });
    try {
      const model = await seedCodexDefaultPermissionPreferences(page, seeded.repoPath);
      const agent = await seeded.client.createAgent({
        provider: "codex",
        cwd: seeded.repoPath,
        workspaceId: seeded.workspaceId,
        title: "Codex live mode preference e2e",
        modeId: "auto",
        model,
      });

      await openAgentRoute(page, {
        workspaceId: seeded.workspaceId,
        agentId: agent.id,
      });
      await expect(
        page.getByRole("button", { name: "Select agent mode (Default permissions)" }),
      ).toBeVisible({ timeout: 30_000 });

      await selectMode(page, "Full access");
      await expect(
        page.getByRole("button", { name: "Select agent mode (Full access)" }),
      ).toBeVisible({ timeout: 30_000 });

      await openGlobalNewWorkspaceComposer(page);
      await selectNewWorkspaceProject(page, {
        projectKey: seeded.projectKey,
        projectDisplayName: seeded.projectDisplayName,
      });

      await expect(
        page.getByRole("button", { name: "Select agent mode (Full access)" }),
      ).toBeVisible({ timeout: 30_000 });
    } finally {
      await seeded.cleanup();
    }
  });
});
