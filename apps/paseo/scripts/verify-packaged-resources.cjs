const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { extractFile, listPackage } = require("@electron/asar");

function verifyServerDirectory(resources) {
  const directory = path.join(resources, "bin");
  assert.deepEqual(
    fs.readdirSync(directory).sort(),
    ["server"],
    "Packaged resources/bin must contain only server",
  );
  const server = path.join(directory, "server");
  assert(fs.lstatSync(server).isFile(), "Bundled server must be a regular file");
  fs.accessSync(server, fs.constants.X_OK);
}

function verifyPackagedResources({ appOutDir, platform, version }) {
  assert(["darwin", "linux"].includes(platform), `Unsupported release platform: ${platform}`);
  const resources =
    platform === "darwin"
      ? path.join(appOutDir, "Ait.app", "Contents", "Resources")
      : path.join(appOutDir, "resources");
  verifyServerDirectory(resources);
  assert(
    fs.existsSync(path.join(resources, "app-dist", "index.html")),
    "Exported apps/app UI is missing",
  );
  const asar = path.join(resources, "app.asar");
  const pkg = JSON.parse(extractFile(asar, "package.json").toString());
  assert.equal(pkg.version, version, "Packaged desktop version differs from release version");
  assert.equal(pkg.name, "@ait/desktop", "Release must use Ait's independent package identity");
  assert(
    !listPackage(asar).some((entry) =>
      /\/node_modules\/@getpaseo\/(?:cli|server)(?:\/|$)/.test(entry),
    ),
    "Legacy Node server/CLI must not be bundled",
  );
  console.log(`Verified Ait ${version}: apps/paseo, exported UI, resources/bin/server only.`);
}

module.exports = { verifyPackagedResources, verifyServerDirectory };
