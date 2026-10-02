import { expect, test } from "../../mobile/e2e/support/fixtures";
import { gotoAppShell } from "../../mobile/e2e/support/helpers/app";
import { getServerId } from "../../mobile/e2e/support/helpers/server-id";
import {
  clickCheckForUpdates,
  clickInstallUpdate,
  expectDaemonManagementConfirmDialog,
  expectDaemonManagementDisabled,
  expectDaemonManagementEnabled,
  expectDaemonStatusLogPath,
  expectDaemonStatusPid,
  expectDaemonStatusVersion,
  expectInstallInProgress,
  expectPendingUpdateCheckResult,
  expectReadyUpdateCheckResult,
  expectUpdateBanner,
  installDesktopRuntime,
  interceptDaemonManagementConfirmDialog,
  interceptDaemonStopConfirmDialog,
  loadRealDaemonState,
  openDesktopAboutSettings,
  openDesktopSettings,
  toggleDaemonManagement,
} from "./support/runtime";

// These renderer cases use the Desktop bridge fixture. Actual Electron ownership
// and native confirmation journeys live in rust-startup.e2e.mjs.
test.describe("Desktop updates", () => {
  test("clicking install shows the installing state on the callout", async ({ page }) => {
    await installDesktopRuntime(page, {
      serverId: getServerId(),
      updateAvailable: true,
      latestVersion: "1.2.3",
      slowInstall: true,
    });
    await gotoAppShell(page);

    await expectUpdateBanner(page, "1.2.3");
    await clickInstallUpdate(page);
    await expectInstallInProgress(page);
  });

  test("manual check reports a found update while it downloads", async ({ page }) => {
    await installDesktopRuntime(page, {
      serverId: getServerId(),
      updateAvailable: true,
      latestVersion: "1.2.3",
      updateReadyToInstall: false,
    });
    await gotoAppShell(page);
    await openDesktopAboutSettings(page);

    await clickCheckForUpdates(page);

    await expectPendingUpdateCheckResult(page, "1.2.3");
  });

  test("manual update remains available after the automatic rollout recheck", async ({ page }) => {
    await installDesktopRuntime(page, {
      serverId: getServerId(),
      latestVersion: "1.2.3",
      manualUpdateBypassesRollout: true,
    });
    await gotoAppShell(page);
    await openDesktopAboutSettings(page);

    await clickCheckForUpdates(page);
    await expectPendingUpdateCheckResult(page, "1.2.3");
    await expectReadyUpdateCheckResult(page, "1.2.3");
  });
});

test.describe("Desktop daemon management", () => {
  test("cancelling the management confirmation preserves the enabled daemon", async ({ page }) => {
    const serverId = getServerId();
    await installDesktopRuntime(page, {
      serverId,
      manageBuiltInDaemon: true,
      ownedByDesktop: true,
      confirmShouldAccept: false,
    });
    await gotoAppShell(page);
    await openDesktopSettings(page, serverId);

    const dialogArgs = await interceptDaemonManagementConfirmDialog(page);
    expectDaemonManagementConfirmDialog(dialogArgs);

    await expectDaemonManagementEnabled(page);
  });

  test("confirming the dialog disables built-in daemon management", async ({ page }) => {
    const serverId = getServerId();
    await installDesktopRuntime(page, {
      serverId,
      manageBuiltInDaemon: true,
      ownedByDesktop: true,
      confirmShouldAccept: true,
    });
    await gotoAppShell(page);
    await openDesktopSettings(page, serverId);

    await toggleDaemonManagement(page, "disable");

    await expectDaemonManagementDisabled(page);
  });

  test("daemon status panel renders version, PID, and log path from the real daemon", async ({
    page,
  }) => {
    const serverId = getServerId();
    const realState = await loadRealDaemonState();
    await installDesktopRuntime(page, {
      serverId,
      manageBuiltInDaemon: false,
      daemonPid: realState.pid,
      daemonVersion: realState.version,
      daemonLogPath: realState.logPath,
    });
    await gotoAppShell(page);
    await openDesktopSettings(page, serverId);

    await expectDaemonStatusVersion(page, realState.version);
    await expectDaemonStatusPid(page, realState.pid);
    await expectDaemonStatusLogPath(page, realState.logPath);
  });

  for (const confirmShouldAccept of [false, true]) {
    test(`${confirmShouldAccept ? "confirming" : "cancelling"} Stop identifies the owned daemon`, async ({
      page,
    }) => {
      const serverId = getServerId();
      const realState = await loadRealDaemonState();
      const daemonHome = process.env.E2E_AIT_DATA_DIR!;
      await installDesktopRuntime(page, {
        serverId,
        daemonPid: realState.pid,
        daemonHome,
        ownedByDesktop: true,
        confirmShouldAccept,
      });
      await gotoAppShell(page);
      await openDesktopSettings(page, serverId);

      const dialog = await interceptDaemonStopConfirmDialog(page);
      expect(dialog).toEqual({
        title: "Stop local daemon?",
        message: [
          "This daemon was launched by this Desktop session.",
          `Home: ${daemonHome}`,
          `Supervisor PID: ${realState.pid}`,
          "Running agent work will be interrupted.",
        ].join("\n"),
      });
      await expectDaemonStatusPid(page, confirmShouldAccept ? null : realState.pid);
    });
  }
});
