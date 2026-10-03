import { expect, test } from "../support/fixtures";
import { TerminalE2EHarness } from "../support/helpers/terminal-dsl";
import { getTerminalBufferText } from "../support/helpers/terminal-perf";

let harness: TerminalE2EHarness;

test.beforeEach(async () => {
  harness = await TerminalE2EHarness.create({ tempPrefix: "terminal-selection-" });
});

test.afterEach(async () => harness.cleanup());

test("keeps double-click and mouse-drag selections after terminal size synchronization", async ({
  context,
  page,
}) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  const terminal = await harness.createTerminal({
    name: "Selection",
    command: "sh",
    args: ["-c", "printf 'select this terminal text\\n'; cat"],
  });
  await harness.openTerminal(page, { terminalId: terminal.id });
  await expect.poll(() => getTerminalBufferText(page)).toContain("select this terminal text");
  const screen = page
    .getByTestId("terminal-surface")
    .filter({ visible: true })
    .locator(".xterm-screen");
  const metrics = await screen.evaluate((element) => {
    const bounds = element.getBoundingClientRect();
    const term = (window as Window & { __paseoTerminal: { rows: number; cols: number } })
      .__paseoTerminal;
    return {
      x: bounds.x,
      y: bounds.y,
      width: bounds.width / term.cols,
      height: bounds.height / term.rows,
    };
  });
  const selection = () =>
    page.evaluate(() =>
      (
        window as Window & { __paseoTerminal: { getSelection(): string } }
      ).__paseoTerminal.getSelection(),
    );
  await screen.dblclick({ position: { x: metrics.width * 8.5, y: metrics.height / 2 } });
  await expect.poll(selection).toBe("this");
  await page.waitForTimeout(500);
  expect(await selection()).toBe("this");
  await page.keyboard.press("ControlOrMeta+c");
  await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe("this");
  await page.mouse.move(metrics.x + metrics.width * 7.2, metrics.y + metrics.height / 2);
  await page.mouse.down();
  await page.mouse.move(metrics.x + metrics.width * 11.2, metrics.y + metrics.height / 2, {
    steps: 10,
  });
  await page.waitForTimeout(300);
  await page.mouse.up();
  expect(await selection()).toBe("this");
  await page.waitForTimeout(500);
  expect(await selection()).toBe("this");
  await page.keyboard.press("ControlOrMeta+c");
  await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe("this");
});
