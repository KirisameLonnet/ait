export interface DesktopStartupDependencies {
  inheritLoginShellEnv: () => void;
  bootstrapGui: () => Promise<void>;
}

export async function runDesktopStartup(deps: DesktopStartupDependencies): Promise<void> {
  deps.inheritLoginShellEnv();
  await deps.bootstrapGui();
}
