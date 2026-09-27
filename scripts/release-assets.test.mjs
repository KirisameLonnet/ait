import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { stringify, parse } from "yaml";
import { collectReleaseAssets, releaseAssetNames, verifyReleaseAssets } from "./release-assets.mjs";

async function fixture(t, platform) {
  const root = await mkdtemp(path.join(tmpdir(), "ait-release-assets-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const source = path.join(root, "source");
  const destination = path.join(root, "release");
  await mkdir(source);
  const version = "0.0.7";
  const names = releaseAssetNames(platform, version);
  const files = [];
  for (const name of names.slice(0, 2)) {
    const contents = Buffer.from(`packaged installer: ${name}`);
    await writeFile(path.join(source, name), contents);
    files.push({
      url: name,
      sha512: createHash("sha512").update(contents).digest("base64"),
      size: contents.length,
    });
  }
  await writeFile(path.join(source, names[2]), stringify({ version, files }));
  await writeFile(path.join(source, "old-server.exe"), "should never be collected");
  return { platform, version, source, destination };
}

test("collects both platforms, updater metadata and blockmaps, then checksums every asset", async (t) => {
  const linux = await fixture(t, "linux");
  const mac = await fixture(t, "mac");
  const blockmap = "Ait-0.0.7-macos-arm64.zip.blockmap";
  await writeFile(path.join(mac.source, blockmap), "blockmap");
  await collectReleaseAssets(linux);
  await collectReleaseAssets({ ...mac, destination: linux.destination });
  const names = await verifyReleaseAssets({ version: linux.version, directory: linux.destination });
  assert.equal(names.length, 7);
  assert(names.includes(blockmap));
  const lines = (await readFile(path.join(linux.destination, "SHA256SUMS"), "utf8"))
    .trim()
    .split("\n");
  assert.equal(lines.length, names.length);
  for (const name of names) {
    const hash = createHash("sha256")
      .update(await readFile(path.join(linux.destination, name)))
      .digest("hex");
    assert(lines.includes(`${hash}  ${name}`));
  }
  assert(!(await readdir(linux.destination)).includes("old-server.exe"));
});

test("rejects stale updater checksums and missing installers", async (t) => {
  const input = await fixture(t, "linux");
  await writeFile(path.join(input.source, "Ait-linux-x64.AppImage"), "changed");
  await assert.rejects(collectReleaseAssets(input), /checksum mismatch/);
  await rm(path.join(input.source, "Ait-0.0.7-linux-x64.tar.gz"));
  await assert.rejects(collectReleaseAssets(input), /ENOENT/);
});

test("rejects incomplete releases and unintended platform assets", async (t) => {
  const input = await fixture(t, "linux");
  await collectReleaseAssets(input);
  await assert.rejects(
    verifyReleaseAssets({ version: input.version, directory: input.destination }),
    /Missing release asset/,
  );
  const mac = await fixture(t, "mac");
  await collectReleaseAssets({ ...mac, destination: input.destination });
  await writeFile(path.join(input.destination, "Ait-Setup.exe"), "unexpected");
  await assert.rejects(
    verifyReleaseAssets({ version: input.version, directory: input.destination }),
    /Unexpected release asset/,
  );
});

test("release workflow builds only server and packages apps/paseo on the supported runners", async () => {
  const workflow = parse(
    await readFile(new URL("../.github/workflows/release.yml", import.meta.url), "utf8"),
  );
  assert.deepEqual(
    workflow.jobs.build.strategy.matrix.include.map((entry) => entry.platform).sort(),
    ["linux", "mac"],
  );
  const build = workflow.jobs.build.steps.find((step) => step.name === "Build release server only");
  assert.match(build.run, /--locked --release -p server-bin --bin server --target/);
  const config = parse(
    await readFile(new URL("../apps/paseo/electron-builder.yml", import.meta.url), "utf8"),
  );
  assert.equal(config.appId, "dev.ait.desktop");
  assert.deepEqual(
    config.extraResources.filter((entry) => entry.to.startsWith("bin")),
    [{ from: "release-resources/server/server", to: "bin/server" }],
  );
  assert.deepEqual(config.mac.binaries, ["Contents/Resources/bin/server"]);
  assert.equal(config.win, undefined);
  assert.deepEqual(config.linux.target, ["AppImage", "tar.gz"]);
});
