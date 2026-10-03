import { beforeEach, describe, expect, it, vi } from "vitest";
import { openExternalUrl } from "./open-external-url";

const openUrl = vi.hoisted(() => vi.fn(async (_url: string) => {}));
vi.mock("@/constants/platform", () => ({ isWeb: true }));
vi.mock("@/desktop/host", () => ({ getDesktopHost: () => ({ opener: { openUrl } }) }));

beforeEach(() => {
  openUrl.mockClear();
});

describe("external URLs", () => {
  it("opens the email contact through the desktop host", async () => {
    await openExternalUrl("mailto:dong@necoex.com");
    expect(openUrl).toHaveBeenCalledWith("mailto:dong@necoex.com");
  });

  it.each(["file:///private/data", "javascript:alert(1)", "/relative", "invalid"])(
    "safely ignores %s for fire-and-forget callers",
    async (url) => {
      await expect(openExternalUrl(url)).resolves.toBeUndefined();
      expect(openUrl).not.toHaveBeenCalled();
    },
  );
});
