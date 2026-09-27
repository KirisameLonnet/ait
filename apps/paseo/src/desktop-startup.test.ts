import { describe, expect, it } from "vitest";
import { runDesktopStartup } from "./desktop-startup";

describe("desktop startup", () => {
  it("inherits the login environment before starting the Ait GUI", async () => {
    const calls: string[] = [];
    await runDesktopStartup({
      inheritLoginShellEnv: () => {
        calls.push("env");
      },
      bootstrapGui: async () => {
        calls.push("gui");
      },
    });
    expect(calls).toEqual(["env", "gui"]);
  });
});
