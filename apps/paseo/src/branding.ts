import { mkdirSync } from "node:fs";
import type { App } from "electron";

export function setDesktopDisplayName(
  app: Pick<App, "getPath" | "setPath" | "setName">,
  name: string,
): void {
  // Pin the already selected legacy, worktree or custom profile before renaming.
  // Electron otherwise derives new storage paths from the new display name.
  const userData = app.getPath("userData");
  const sessionData = app.getPath("sessionData");
  app.setName(name);
  for (const [key, value] of [
    ["userData", userData],
    ["sessionData", sessionData],
  ] as const) {
    mkdirSync(value, { recursive: true });
    app.setPath(key, value);
  }
}
