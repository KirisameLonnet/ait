import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, it } from "vitest";
import { setDesktopDisplayName } from "./branding.js";

it.each(["Paseo", "Paseo-feature", "custom-profile"])(
  "retains the %s profile and Chromium session when the application is renamed",
  (profile) => {
    const root = mkdtempSync(join(tmpdir(), "ait-branding-"));
    const original = {
      userData: join(root, profile),
      sessionData: join(root, profile, "browser-session"),
    };
    const paths = { ...original };
    let name = "Paseo";
    try {
      setDesktopDisplayName(
        {
          getPath: (key) => paths[key as keyof typeof paths],
          setPath: (key, value) => {
            paths[key as keyof typeof paths] = value;
          },
          setName: (value) => {
            name = value;
            // Model Electron deriving fresh paths from a changed product name.
            paths.userData = join(root, value);
            paths.sessionData = join(root, value);
          },
        },
        "Ait",
      );
      expect(name).toBe("Ait");
      expect(paths).toEqual(original);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  },
);
