import { readFile } from "node:fs/promises";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import path from "node:path";
import { expect, test } from "../support/fixtures";
import { repoRoot, requireAitServer } from "../support/helpers/ait-server";
import { getE2EDaemonPort } from "../support/helpers/daemon-port";
import { createTempGitRepo } from "../support/helpers/workspace";
import {
  addMaestroProjects,
  connectMaestroClient,
  removeMaestroProjects,
} from "../../maestro/support/ait-client";

test("Maestro helpers authenticate, prepare a project, open it and remove their receipt", async () => {
  const server = requireAitServer(Number(getE2EDaemonPort()));
  const environment = {
    AIT_MAESTRO_SERVER_URL: `http://127.0.0.1:${server.port}`,
    AIT_MAESTRO_TOKEN: server.token,
  };
  const runCli = (command: string) =>
    promisify(execFile)(
      process.execPath,
      ["--import", "tsx", "apps/app/maestro/support/cli.ts", command],
      {
        cwd: repoRoot,
        env: { ...process.env, ...environment },
        timeout: 15_000,
      },
    );
  await runCli("check");
  expect((await runCli("reverse-port")).stdout.trim()).toBe(String(server.port));
  await expect(
    connectMaestroClient({ ...environment, AIT_MAESTRO_TOKEN: "wrong-test-token" }),
  ).rejects.toThrow("HTTP 401");
  const repo = await createTempGitRepo("ait-maestro-");
  const client = await connectMaestroClient(environment);
  const receipt = path.join(repo.path, "maestro-projects.json");
  try {
    const ids = await addMaestroProjects(client, [repo.path], receipt);
    expect(ids).toHaveLength(1);
    expect(JSON.parse(await readFile(receipt, "utf8"))).toEqual(ids);
    const opened = await client.openProject(repo.path);
    expect(opened.error).toBeNull();
    expect(opened.workspace?.projectId).toBe(ids[0]);
    await removeMaestroProjects(client, receipt);
    expect(
      (await client.listProjects()).projects.some((project) => project.projectId === ids[0]),
    ).toBe(false);
  } finally {
    await removeMaestroProjects(client, receipt).catch(() => undefined);
    await client.close();
    await repo.cleanup();
  }
});
