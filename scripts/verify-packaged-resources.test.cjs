const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");
const { verifyServerDirectory } = require("../apps/paseo/scripts/verify-packaged-resources.cjs");

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
