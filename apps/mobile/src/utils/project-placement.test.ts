import { describe, expect, it } from "vitest";
import { deriveProjectPlacementFromCwd, resolveProjectPlacement } from "./project-placement";

describe("project-placement", () => {
  it("derives fallback placement from cwd", () => {
    const placement = deriveProjectPlacementFromCwd("/Users/test/repo");

    expect(placement.projectKey).toBe("/Users/test/repo");
    expect(placement.projectName).toBe("repo");
    expect(placement.checkout.cwd).toBe("/Users/test/repo");
    expect(placement.checkout.isGit).toBe(false);
  });

  it("keeps the current directory when server placement is unavailable", () => {
    const cwd = "/custom/data/worktrees/hash/feature";
    const placement = deriveProjectPlacementFromCwd(cwd);
    expect(placement.projectKey).toBe(cwd);
    expect(placement.projectName).toBe("feature");
    expect(placement.checkout.cwd).toBe(cwd);
  });

  it("prefers an existing placement when present", () => {
    const existing = {
      projectKey: "remote:github.com/acme/repo",
      projectName: "acme/repo",
      checkout: {
        cwd: "/Users/test/repo",
        isGit: true as const,
        currentBranch: "main",
        remoteUrl: "https://github.com/acme/repo.git",
        worktreeRoot: "/Users/test/repo",
        isPaseoOwnedWorktree: false as const,
        mainRepoRoot: null,
      },
    };

    const resolved = resolveProjectPlacement({
      projectPlacement: existing,
      cwd: "/Users/test/repo",
    });

    expect(resolved).toBe(existing);
  });
});
