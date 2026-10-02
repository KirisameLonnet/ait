import { mkdirSync } from "node:fs";
import path from "node:path";
import type { App } from "electron";

export function configureDesktopProfile(
  app: Pick<App, "getPath" | "setPath" | "setName">,
  options: { name?: string; userDataPath?: string; worktreeName?: string | null } = {},
): void {
  const name = options.name ?? "Ait";
  const profileName = options.worktreeName ? `${name}-${options.worktreeName}` : name;
  const userData = options.userDataPath || path.join(app.getPath("appData"), profileName);
  app.setName(name);
  mkdirSync(userData, { recursive: true });
  // Select both paths before Chromium or the single-instance lock is initialized.
  // Never adopt the shared Paseo profile, which may contain another app's state.
  app.setPath("userData", userData);
  app.setPath("sessionData", userData);
}

export function desktopProfileOptions(env: NodeJS.ProcessEnv = process.env) {
  return {
    name: env.AIT_TEST_APP_NAME?.trim() || "Ait",
    userDataPath: env.AIT_ELECTRON_USER_DATA_DIR?.trim() || undefined,
  };
}
