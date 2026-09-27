import { chmodSync, copyFileSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";
import { execFileSync } from "node:child_process";

const desktop = fileURLToPath(new URL("..", import.meta.url));
const root = path.resolve(desktop, "../..");

export function stageServer({ binary, directory, version }) {
  const actual = execFileSync(binary, ["--version"], { encoding: "utf8" }).trim();
  if (actual !== `server ${version}`) {
    throw new Error(`Expected server ${version}, received ${actual}`);
  }
  // This directory is generated packaging input. Never carry old sidecars forward.
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(directory, { recursive: true });
  const staged = path.join(directory, "server");
  copyFileSync(binary, staged);
  chmodSync(staged, 0o755);
  return staged;
}

function main() {
  if (process.platform !== "darwin" && process.platform !== "linux") {
    throw new Error("Ait desktop releases support macOS and Linux only.");
  }
  const binary = process.env.AIT_SERVER_BIN || path.join(root, "target/release/server");
  if (!process.env.AIT_SERVER_BIN) {
    execFileSync(
      "cargo",
      [
        "build",
        "--locked",
        "--release",
        "--target-dir",
        path.join(root, "target"),
        "-p",
        "server-bin",
        "--bin",
        "server",
      ],
      { cwd: root, stdio: "inherit" },
    );
  }
  const { version } = JSON.parse(readFileSync(path.join(desktop, "package.json"), "utf8"));
  const staged = stageServer({
    binary: path.resolve(binary),
    directory: path.join(desktop, "release-resources/server"),
    version,
  });
  console.log(`Staged server ${version}: ${staged}`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href)
  main();
