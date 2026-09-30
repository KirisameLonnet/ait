import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { parseAllDocuments, stringify } from "yaml";

/** Render only the referenced flow tree, preserving YAML types and quoting input values. */
export async function renderMaestroFlows(
  template: string,
  outputDirectory: string,
  variables: Readonly<Record<string, string | undefined>>,
) {
  const root = path.dirname(template);
  const visited = new Set<string>();
  const references: string[] = [];
  function resolve(value: unknown): unknown {
    if (typeof value === "string") {
      return value.replace(/\$\{(AIT_MAESTRO_[A-Z_]+)\}/g, (_match, name: string) => {
        const replacement = variables[name];
        if (replacement === undefined) throw new Error(`Missing flow variable ${name}`);
        return replacement;
      });
    }
    if (Array.isArray(value)) return value.map(resolve);
    if (value && typeof value === "object") {
      const result = Object.fromEntries(
        Object.entries(value).map(([key, item]) => [key, resolve(item)]),
      );
      const flow = result.runFlow;
      if (typeof flow === "string") references.push(flow);
      else if (
        flow &&
        typeof flow === "object" &&
        "file" in flow &&
        typeof flow.file === "string"
      ) {
        references.push(flow.file);
      }
      return result;
    }
    return value;
  }
  async function render(relative: string) {
    const source = path.resolve(root, relative);
    if (!source.startsWith(`${root}${path.sep}`))
      throw new Error("Flow reference escapes its directory");
    if (visited.has(source)) return;
    visited.add(source);
    const documents = parseAllDocuments(await readFile(source, "utf8"));
    if (documents.some((document) => document.errors.length > 0))
      throw new Error(`Invalid flow: ${relative}`);
    const values = documents.map((document) => resolve(document.toJS()));
    const config = values[0];
    if (config && typeof config === "object" && "appId" in config) {
      config.appId = variables.AIT_MAESTRO_APP_ID ?? "dev.ait.mobile.debug";
    }
    const nested = references.splice(0);
    const destination = path.join(outputDirectory, path.relative(root, source));
    await mkdir(path.dirname(destination), { recursive: true, mode: 0o700 });
    await writeFile(destination, values.map((value) => stringify(value)).join("---\n"), {
      mode: 0o600,
    });
    for (const next of nested) await render(path.join(path.dirname(relative), next));
  }
  await render(path.basename(template));
  return path.join(outputDirectory, path.basename(template));
}
