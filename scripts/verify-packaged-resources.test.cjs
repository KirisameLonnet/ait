const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");
const { createPackage, uncache } = require("@electron/asar");
const {
  verifyPackagedResources,
  verifyServerDirectory,
} = require("../apps/paseo/scripts/verify-packaged-resources.cjs");

function resources(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "ait-package-resources-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.mkdirSync(path.join(root, "bin"));
  fs.writeFileSync(path.join(root, "bin", "server"), "server");
  fs.chmodSync(path.join(root, "bin", "server"), 0o755);
  return root;
}

test("the packaged bin directory allows exactly one executable server", (t) => {
  assert.doesNotThrow(() => verifyServerDirectory(resources(t)));
});

test("packaging fails if an old binary or CLI shim enters resources/bin", (t) => {
  for (const name of ["ait-daemon", "ait-worker", "ait", "paseo"]) {
    const root = resources(t);
    fs.writeFileSync(path.join(root, "bin", name), "unwanted");
    assert.throws(() => verifyServerDirectory(root), /only server/);
  }
});

test("packaging rejects a missing or non-executable server", (t) => {
  const root = resources(t);
  fs.chmodSync(path.join(root, "bin", "server"), 0o644);
  assert.throws(() => verifyServerDirectory(root));
  fs.unlinkSync(path.join(root, "bin", "server"));
  assert.throws(() => verifyServerDirectory(root), /only server/);
});

test("packaging requires Ait's independent identity instead of the Paseo workspace name", async (t) => {
  const root = resources(t);
  const appOutDir = path.join(root, "packaged");
  const destination = path.join(appOutDir, "resources");
  fs.mkdirSync(path.join(destination, "app-dist"), { recursive: true });
  fs.writeFileSync(path.join(destination, "app-dist", "index.html"), "<!doctype html>");
  fs.cpSync(path.join(root, "bin"), path.join(destination, "bin"), { recursive: true });
  const source = path.join(root, "source");
  fs.mkdirSync(source);
  for (const name of ["@ait/desktop", "@getpaseo/desktop"]) {
    fs.writeFileSync(path.join(source, "package.json"), JSON.stringify({ name, version: "0.0.7" }));
    await createPackage(source, path.join(destination, "app.asar"));
    uncache(path.join(destination, "app.asar"));
    const verify = () =>
      verifyPackagedResources({ appOutDir, platform: "linux", version: "0.0.7" });
    if (name === "@ait/desktop") assert.doesNotThrow(verify);
    else assert.throws(verify, /independent package identity/);
  }
});
