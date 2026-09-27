import { deriveProjectName } from "@/utils/agent-grouping";
import type { ProjectPlacementPayload } from "@getpaseo/protocol/messages";

function normalizeWorkingDirectory(cwd: string): string {
  const trimmed = cwd.trim();
  return trimmed.length > 0 ? trimmed : ".";
}

export function deriveProjectPlacementFromCwd(cwd: string): ProjectPlacementPayload {
  const normalizedCwd = normalizeWorkingDirectory(cwd);
  const projectKey = normalizedCwd;

  return {
    projectKey,
    projectName: deriveProjectName(projectKey),
    workspaceName: null,
    checkout: {
      cwd: normalizedCwd,
      isGit: false,
      currentBranch: null,
      remoteUrl: null,
      worktreeRoot: null,
      isPaseoOwnedWorktree: false,
      mainRepoRoot: null,
    },
  };
}

export function resolveProjectPlacement(input: {
  projectPlacement: ProjectPlacementPayload | null | undefined;
  cwd: string;
}): ProjectPlacementPayload {
  return input.projectPlacement ?? deriveProjectPlacementFromCwd(input.cwd);
}
