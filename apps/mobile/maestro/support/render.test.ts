import { mkdtemp, readFile, readdir, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { parseAllDocuments } from "yaml";
import { renderMaestroFlows } from "./render";

const directories: string[] = [];
afterEach(async () => {
  await Promise.all(directories.splice(0).map((dir) => rm(dir, { recursive: true, force: true })));
});

describe("native flow rendering", () => {
  it.each(["workspace-create-android-crash.yaml", "sidebar-drag-cancellation-regression.yaml"])(
    "renders %s and its referenced flows as valid YAML",
    async (name) => {
      const output = await mkdtemp(path.join(tmpdir(), "ait-maestro-render-"));
      directories.push(output);
      const variables = {
        AIT_MAESTRO_APP_ID: "dev.ait.mobile.debug",
        AIT_MAESTRO_CONNECTION_URI: 'ws://localhost:41234?token="test: token"',
        AIT_MAESTRO_PROJECT_NAME: 'project: "quoted"',
        AIT_MAESTRO_DRAG_A_NAME: "first",
        AIT_MAESTRO_DRAG_B_NAME: "second",
        AIT_MAESTRO_DRAG_Z_NAME: "last",
      };
      const entry = await renderMaestroFlows(
        path.resolve(__dirname, "..", name),
        output,
        variables,
      );
      expect(entry).toBe(path.join(output, name));
      const files = [
        name,
        ...(await readdir(path.join(output, "flows"))).map((file) => `flows/${file}`),
      ];
      for (const file of files) {
        const source = await readFile(path.join(output, file), "utf8");
        expect(source).not.toMatch(/\$\{AIT_MAESTRO_/);
        const docs = parseAllDocuments(source);
        expect(docs.every((doc) => doc.errors.length === 0)).toBe(true);
        expect(docs[0].toJS().appId).toBe(variables.AIT_MAESTRO_APP_ID);
        expect((await stat(path.join(output, file))).mode & 0o777).toBe(0o600);
      }
      const connect = parseAllDocuments(
        await readFile(path.join(output, "flows/connect-direct-if-welcome.yaml"), "utf8"),
      )[1].toJS();
      expect(connect[0].runFlow.commands).toContainEqual({
        inputText: variables.AIT_MAESTRO_CONNECTION_URI,
      });
    },
  );

  it("rejects missing flow variables before starting native automation", async () => {
    const output = await mkdtemp(path.join(tmpdir(), "ait-maestro-render-"));
    directories.push(output);
    await expect(
      renderMaestroFlows(
        path.resolve(__dirname, "../workspace-create-android-crash.yaml"),
        output,
        { AIT_MAESTRO_APP_ID: "dev.ait.mobile.debug" },
      ),
    ).rejects.toThrow("Missing flow variable");
  });
});
