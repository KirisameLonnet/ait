import { collectAgentWorkingDirectorySuggestions } from "@/utils/agent-working-directory-suggestions";
import { deriveProjectPlacementFromCwd } from "@/utils/project-placement";
import { describe, expect, it } from "vitest";

describe("collectAgentWorkingDirectorySuggestions", () => {
  it("deduplicates by cwd and sorts by most recent timestamp", () => {
    const results = collectAgentWorkingDirectorySuggestions([
      {
        cwd: "/Users/me/project-alpha",
        createdAt: new Date("2026-02-10T10:00:00.000Z"),
      },
      {
        cwd: "/Users/me/project-beta",
        createdAt: new Date("2026-02-11T10:00:00.000Z"),
      },
      {
        cwd: "/Users/me/project-alpha",
        lastActivityAt: new Date("2026-02-12T10:00:00.000Z"),
      },
    ]);

    expect(results).toEqual(["/Users/me/project-alpha", "/Users/me/project-beta"]);
  });

  it("excludes server-owned worktrees using placement metadata", () => {
    const results = collectAgentWorkingDirectorySuggestions([
      {
        cwd: "/custom/data/worktrees/hash/feature-a",
        projectPlacement: {
          ...deriveProjectPlacementFromCwd("/Users/me/repo"),
          checkout: {
            ...deriveProjectPlacementFromCwd("/Users/me/repo").checkout,
            isGit: true,
            worktreeRoot: "/custom/worktree",
            mainRepoRoot: "/repo",
            isPaseoOwnedWorktree: true,
          },
        },
        createdAt: new Date("2026-02-12T10:00:00.000Z"),
      },
      {
        cwd: "/Users/me/repo",
        createdAt: new Date("2026-02-10T10:00:00.000Z"),
      },
      {
        cwd: "D:\\custom\\worktrees\\feature-b",
        projectPlacement: {
          ...deriveProjectPlacementFromCwd("C:\\repo"),
          checkout: {
            ...deriveProjectPlacementFromCwd("C:\\repo").checkout,
            isGit: true,
            worktreeRoot: "/custom/worktree",
            mainRepoRoot: "/repo",
            isPaseoOwnedWorktree: true,
          },
        },
        createdAt: new Date("2026-02-11T10:00:00.000Z"),
      },
    ]);

    expect(results).toEqual(["/Users/me/repo"]);
  });

  it("does not infer ownership from a directory name", () => {
    const cwd = "/custom/.ait-server/worktrees/example";
    expect(collectAgentWorkingDirectorySuggestions([{ cwd }])).toEqual([cwd]);
  });

  it("ignores empty cwd values", () => {
    const results = collectAgentWorkingDirectorySuggestions([
      { cwd: "   ", createdAt: new Date("2026-02-10T10:00:00.000Z") },
      { cwd: null, createdAt: new Date("2026-02-11T10:00:00.000Z") },
      { cwd: undefined, lastActivityAt: new Date("2026-02-12T10:00:00.000Z") },
      {
        cwd: "/Users/me/project",
        createdAt: new Date("2026-02-09T10:00:00.000Z"),
      },
    ]);

    expect(results).toEqual(["/Users/me/project"]);
  });
});
