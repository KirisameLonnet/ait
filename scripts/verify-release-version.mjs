import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";

const root = fileURLToPath(new URL("../", import.meta.url));
const read = (path) => readFile(resolve(root, path), "utf8");
const cargo = await read("Cargo.toml");
const workspacePackage = cargo.match(/\[workspace\.package\]([\s\S]*?)(?:\n\[|$)/)?.[1];
const version = workspacePackage?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
if (!version) throw new Error("Could not read workspace.package.version from Cargo.toml");

const tag = process.argv[2] ?? `v${version}`;
if (!/^v(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)$/.test(tag)) {
  throw new Error(`Release tag must use semantic version form vX.Y.Z: ${tag}`);
}
if (tag !== `v${version}`) throw new Error(`Release tag ${tag} differs from Ait ${version}`);

const workspace = JSON.parse(await read("package.json"));
const lock = JSON.parse(await read("package-lock.json"));
const problems = [];
function check(label, actual) {
  if (actual !== version) problems.push(`${label}=${actual}`);
}
check("package-lock.json", lock.version);
const packages = ["", ...workspace.workspaces];
for (const directory of packages) {
  const path = directory ? `${directory}/package.json` : "package.json";
  const pkg = JSON.parse(await read(path));
  check(path, pkg.version);
  check(`package-lock.json:${directory}`, lock.packages[directory]?.version);
}
for (const directory of ["packages/expo-two-way-audio"]) {
  const nested = JSON.parse(await read(`${directory}/package-lock.json`));
  check(`${directory}/package-lock.json`, nested.version);
  check(`${directory}/package-lock.json:root`, nested.packages[""]?.version);
}
if (problems.length) throw new Error(`Expected Ait ${version}:\n${problems.join("\n")}`);
console.log(
  `Ait ${version}: release tag, Rust workspace, ${packages.length} local packages and lockfiles agree.`,
);
