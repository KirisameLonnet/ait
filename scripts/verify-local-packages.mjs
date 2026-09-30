import assert from "node:assert/strict";
import { readFile, realpath } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const readJson = async (file) => JSON.parse(await readFile(path.join(root, file), "utf8"));
const manifest = await readJson("package.json");
const lock = await readJson("package-lock.json");
const workspaces = new Map();

for (const directory of manifest.workspaces) {
  const pkg = await readJson(`${directory}/package.json`);
  assert(pkg.name.startsWith("@ait/"), `Unexpected workspace identity in ${directory}`);
  assert.equal(pkg.private, true, `${pkg.name} must remain repository-local`);
  assert(!workspaces.has(pkg.name), `Duplicate local package ${pkg.name}`);
  workspaces.set(pkg.name, { directory, pkg });
  assert.deepEqual(
    lock.packages[`node_modules/${pkg.name}`],
    { resolved: directory, link: true },
    `${pkg.name} must resolve to the checked-in workspace`,
  );
  assert.equal(lock.packages[directory].name, pkg.name, `Stale lockfile entry for ${pkg.name}`);
}

for (const { directory, pkg } of workspaces.values()) {
  for (const field of [
    "dependencies",
    "devDependencies",
    "optionalDependencies",
    "peerDependencies",
  ]) {
    for (const [name, version] of Object.entries(pkg[field] ?? {})) {
      assert(!name.startsWith("@getpaseo/"), `Retired dependency in ${directory}: ${name}`);
      if (!name.startsWith("@ait/")) continue;
      const local = workspaces.get(name);
      assert(local, `Unknown Ait dependency in ${directory}: ${name}`);
      assert(
        version.startsWith("file:"),
        `${directory} must use an explicit file: dependency for ${name}`,
      );
      const resolved = await realpath(path.resolve(root, directory, version.slice(5)));
      assert.equal(
        resolved,
        await realpath(path.join(root, local.directory)),
        `Incorrect source path for ${name}`,
      );
    }
  }
}

assert(
  !Object.keys(lock.packages).some(
    (entry) =>
      entry.includes("node_modules/@getpaseo/") ||
      ["packages/relay", "packages/plugin"].includes(entry),
  ),
  "Retired Paseo packages remain in the lockfile",
);
console.log(
  `Verified ${workspaces.size} private @ait workspaces and their local dependency links.`,
);
