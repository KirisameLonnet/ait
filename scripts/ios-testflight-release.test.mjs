import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { decideBuild, planRelease } from "./ios-testflight-release.mjs";

const release = { tag: "v0.0.11", pkgVersion: "0.0.11" };
const buildNumber = "11999";

function build(overrides = {}) {
  return {
    id: "build-1",
    platform: "IOS",
    buildProfile: "ait",
    appVersion: "0.0.11",
    appBuildVersion: buildNumber,
    status: "finished",
    ...overrides,
  };
}

test("plans the stable tag with the native iOS build number", () => {
  assert.deepEqual(planRelease(release), {
    tag: "v0.0.11",
    appVersion: "0.0.11",
    buildNumber,
  });
});

test("rejects beta and rc tags and mismatched package versions", () => {
  assert.throws(() => planRelease({ tag: "v0.0.11-beta.1", pkgVersion: "0.0.11" }), /stable/);
  assert.throws(() => planRelease({ tag: "v0.0.11-rc.1", pkgVersion: "0.0.11" }), /stable/);
  assert.throws(() => planRelease({ tag: "v0.0.12", pkgVersion: "0.0.11" }), /differs/);
});

test("fails when a finished build already exists and includes its build ID", () => {
  assert.deepEqual(decideBuild({ builds: [build()], buildNumber }), {
    action: "fail",
    reason: "existing-build",
    buildId: "build-1",
  });
});

test("treats all active and finished statuses as unsafe duplicates", () => {
  for (const status of ["FINISHED", "IN_QUEUE", "IN_PROGRESS", "NEW", "PENDING_CANCEL"]) {
    assert.equal(decideBuild({ builds: [build({ status })], buildNumber }).action, "fail", status);
  }
});

test("fails closed when EAS returns an unfamiliar build status", () => {
  assert.deepEqual(
    decideBuild({ builds: [build({ status: "WAITING_FOR_RESOURCE" })], buildNumber }),
    {
      action: "fail",
      reason: "existing-build",
      buildId: "build-1",
    },
  );
});

test("CLI status treats EAS GraphQL active states as pending", () => {
  const directory = mkdtempSync(join(tmpdir(), "ait-ios-status-"));
  const view = join(directory, "build.json");
  try {
    for (const status of ["IN_QUEUE", "IN_PROGRESS", "PENDING_CANCEL"]) {
      writeFileSync(view, JSON.stringify({ status }));
      assert.equal(
        execFileSync(
          process.execPath,
          ["scripts/ios-testflight-release.mjs", "status", "--view-json", view],
          {
            encoding: "utf8",
          },
        ),
        "pending\n",
      );
    }
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("allows a new build when matching builds only errored or canceled", () => {
  assert.deepEqual(
    decideBuild({
      builds: [build({ status: "errored" }), build({ id: "build-2", status: "canceled" })],
      buildNumber,
    }),
    { action: "build", buildNumber },
  );
});

test("reuses a valid explicit finished build for submission", () => {
  assert.deepEqual(
    decideBuild({ builds: [build()], buildNumber, appVersion: "0.0.11", buildId: "build-1" }),
    {
      action: "reuse",
      buildId: "build-1",
      buildNumber,
    },
  );
});

test("reuses a valid explicit build for a future stable release", () => {
  const future = planRelease({ tag: "v0.0.12", pkgVersion: "0.0.12" });
  const futureBuild = build({
    appVersion: future.appVersion,
    appBuildVersion: future.buildNumber,
    status: "FINISHED",
  });

  assert.deepEqual(
    decideBuild({
      builds: [futureBuild],
      buildNumber: future.buildNumber,
      appVersion: future.appVersion,
      buildId: futureBuild.id,
    }),
    { action: "reuse", buildId: futureBuild.id, buildNumber: future.buildNumber },
  );
});

test("rejects an explicit build that does not match the release contract", () => {
  assert.deepEqual(
    decideBuild({
      builds: [build({ buildProfile: "default", appVersion: "0.0.10" })],
      buildNumber,
      appVersion: "0.0.11",
      buildId: "build-1",
    }),
    { action: "reject", reason: "explicit-build-mismatch", buildId: "build-1" },
  );
});

test("CLI plan writes the derived release outputs", () => {
  const directory = mkdtempSync(join(tmpdir(), "ait-ios-plan-"));
  const output = join(directory, "github-output");
  const packageJson = join(directory, "package.json");
  writeFileSync(packageJson, JSON.stringify({ version: "0.0.11" }));

  try {
    execFileSync(
      process.execPath,
      [
        "scripts/ios-testflight-release.mjs",
        "plan",
        "--tag",
        "v0.0.11",
        "--package-json",
        packageJson,
      ],
      { env: { ...process.env, GITHUB_OUTPUT: output } },
    );
    assert.equal(readFileSync(output, "utf8"), "app_version=0.0.11\nios_build_number=11999\n");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
