import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, it } from "vitest";
import { configureDesktopProfile, desktopProfileOptions } from "./branding.js";

it.each([
  { options: {}, expected: "Ait", name: "Ait" },
  { options: { worktreeName: "feature" }, expected: "Ait-feature", name: "Ait" },
  { options: { name: "Ait Test" }, expected: "Ait Test", name: "Ait Test" },
  {
    options: { name: "Ait Test", worktreeName: "feature" },
    expected: "Ait Test-feature",
    name: "Ait Test",
  },
  {
    options: { userDataPath: "custom-profile", worktreeName: "feature" },
    expected: "custom-profile",
    name: "Ait",
  },
])(
  "isolates $expected without importing or modifying Paseo data",
  ({ options, expected, name }) => {
    const root = mkdtempSync(join(tmpdir(), "ait-branding-"));
    const legacy = join(root, "Paseo");
    const paths = { appData: root, userData: legacy, sessionData: legacy };
    let appName = "Paseo";
    try {
      mkdirSync(legacy);
      writeFileSync(join(legacy, "desktop-settings.json"), "paseo settings");
      configureDesktopProfile(
        {
          getPath: (key) => paths[key as keyof typeof paths],
          setPath: (key, value) => {
            paths[key as keyof typeof paths] = value;
          },
          setName: (value) => {
            appName = value;
            paths.userData = join(root, value);
            paths.sessionData = paths.userData;
          },
        },
        {
          ...options,
          ...(options.userDataPath ? { userDataPath: join(root, options.userDataPath) } : {}),
        },
      );
      expect(appName).toBe(name);
      expect(paths.userData).toBe(join(root, expected));
      expect(paths.sessionData).toBe(paths.userData);
      expect(existsSync(paths.userData)).toBe(true);
      expect(existsSync(join(paths.userData, "desktop-settings.json"))).toBe(false);
      expect(readFileSync(join(legacy, "desktop-settings.json"), "utf8")).toBe("paseo settings");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  },
);

it("ignores Paseo environment overrides", () => {
  expect(
    desktopProfileOptions({
      PASEO_TEST_APP_NAME: "Paseo",
      PASEO_ELECTRON_USER_DATA_DIR: "/foreign",
    }),
  ).toEqual({ name: "Ait", userDataPath: undefined });
  expect(
    desktopProfileOptions({ AIT_TEST_APP_NAME: "Ait Test", AIT_ELECTRON_USER_DATA_DIR: "/ait" }),
  ).toEqual({ name: "Ait Test", userDataPath: "/ait" });
});
