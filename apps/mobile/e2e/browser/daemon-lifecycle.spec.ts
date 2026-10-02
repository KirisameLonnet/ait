import { randomUUID } from "node:crypto";
import { expect, test } from "../support/fixtures";
import {
  expectDaemonRestartComplete,
  openDaemonOverview,
  readWorkerPid,
  restartDaemonInSettings,
} from "../support/helpers/daemon-lifecycle";
import { startIsolatedHostDaemon } from "../support/helpers/isolated-host-daemon";

test("settings restarts the Ait service within the same process", async ({ page }, testInfo) => {
  const daemon = await startIsolatedHostDaemon(randomUUID());
  try {
    const processPid = daemon.getPid();
    const previousPid = await readWorkerPid(daemon);
    await openDaemonOverview(page, daemon);
    await restartDaemonInSettings(page);
    await expectDaemonRestartComplete(page);
    expect(await readWorkerPid(daemon)).toBe(previousPid);
    expect(daemon.getPid()).toBe(processPid);
    await page.screenshot({ path: testInfo.outputPath("restart-confirmed.png"), fullPage: true });
  } finally {
    await daemon.close();
  }
});
