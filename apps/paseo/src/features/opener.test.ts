import { describe, expect, it } from "vitest";

import { createExternalUrlOpener } from "./opener";

describe("desktop opener", () => {
  it.each(["https://example.com/docs#install", "mailto:dong@necoex.com"])(
    "passes %s to its external owner",
    async (url) => {
      const opened: string[] = [];
      const open = createExternalUrlOpener({
        open: async (url) => {
          opened.push(url);
        },
      });

      await open(url);

      expect(opened).toEqual([url]);
    },
  );

  it("does not hand unsupported schemes or relative URLs to the external owner", async () => {
    const opened: string[] = [];
    const open = createExternalUrlOpener({
      open: async (url) => {
        opened.push(url);
      },
    });

    for (const input of [
      "file:///private/data",
      "javascript:alert(1)",
      "paseo://settings",
      "/docs",
      null,
    ]) {
      await expect(open(input)).rejects.toThrow(
        "Only HTTP(S) and mailto URLs can open externally.",
      );
    }

    expect(opened).toEqual([]);
  });
});
