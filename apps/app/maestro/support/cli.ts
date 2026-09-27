import path from "node:path";
import {
  addMaestroProjects,
  connectMaestroClient,
  readMaestroConnection,
  removeMaestroProjects,
} from "./ait-client";
import { renderMaestroFlows } from "./render";

async function main() {
  const [command, ...args] = process.argv.slice(2);
  if (command === "render") {
    const [template, destination] = args;
    if (!template || !destination)
      throw new Error("render requires a template and output directory");
    const connection = readMaestroConnection();
    await renderMaestroFlows(path.resolve(template), path.resolve(destination), {
      ...process.env,
      AIT_MAESTRO_CONNECTION_URI: connection.connectionUri,
    });
    return;
  }
  if (command === "reverse-port") {
    const connection = readMaestroConnection();
    if (!["127.0.0.1", "localhost", "[::1]"].includes(connection.hostname)) {
      throw new Error("ADB reverse requires a local Ait server URL");
    }
    process.stdout.write(`${connection.port}\n`);
    return;
  }
  const client = await connectMaestroClient();
  try {
    if (command === "check") return;
    if (command === "add-projects") {
      const [receipt, ...paths] = args;
      if (!receipt || paths.length === 0)
        throw new Error("add-projects requires a receipt and paths");
      await addMaestroProjects(client, paths, receipt);
    } else if (command === "remove-projects") {
      if (!args[0]) throw new Error("remove-projects requires a receipt");
      await removeMaestroProjects(client, args[0]);
    } else if (command === "open-project") {
      if (!args[0]) throw new Error("open-project requires a path");
      const result = await client.openProject(args[0]);
      if (result.error || !result.workspace)
        throw new Error(result.error ?? "Missing Ait workspace");
      console.log(
        JSON.stringify({ workspaceId: result.workspace.id, projectId: result.workspace.projectId }),
      );
    } else {
      throw new Error(
        "Expected check, add-projects, remove-projects, open-project, render or reverse-port",
      );
    }
  } finally {
    await client.close();
  }
}

main().catch((error: unknown) => {
  // Never print the configured connection URI or token, even on transport failure.
  const message = error instanceof Error ? error.message : String(error);
  const token = process.env.AIT_MAESTRO_TOKEN;
  console.error(token ? message.replaceAll(token, "[redacted]") : message);
  process.exitCode = 1;
});
