import assert from "node:assert/strict";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { stageServer } from "../apps/paseo/scripts/prepare-server.mjs";

function fixture(t, version) {
  const root = mkdtempSync(path.join(tmpdir(), "ait-stage-server-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const binary = path.join(root, "compiled-server");
  writeFileSync(binary, `#!/bin/sh\nprintf 'server ${version}\\n'\n`);
  chmodSync(binary, 0o755);
  return { binary, directory: path.join(root, "staged"), version: "0.0.7" };
}

test("staging removes stale sidecars and copies only the matching server", (t) => {
  const input = fixture(t, "0.0.7");
  mkdirSync(input.directory);
  for (const name of ["ait-daemon", "ait-worker", "paseo", "server.exe"])
    writeFileSync(path.join(input.directory, name), "stale");
  const staged = stageServer(input);
  assert.deepEqual(readdirSync(input.directory), ["server"]);
  assert.deepEqual(readFileSync(staged), readFileSync(input.binary));
});

test("staging rejects an old server even when AIT_SERVER_BIN points to it", (t) => {
  const input = fixture(t, "0.0.6");
  assert.throws(() => stageServer(input), /Expected server 0\.0\.7/);
  assert(!existsSync(input.directory));
});
