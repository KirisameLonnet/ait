const assert = require("node:assert/strict");
const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");
const yaml = require("yaml");

const appDir = path.resolve(__dirname, "..");
const rootDir = path.resolve(appDir, "../..");
const easConfig = JSON.parse(fs.readFileSync(`${appDir}/eas.json`, "utf8"));
const workflowPath = `${rootDir}/.github/workflows/release-ios-testflight.yml`;

function readWorkflow(path) {
  assert.equal(fs.existsSync(path), true, `Expected workflow to exist at ${path}`);
  return {
    parsed: yaml.parse(fs.readFileSync(path, "utf8")),
    source: fs.readFileSync(path, "utf8"),
  };
}

function workflowTriggers(workflow) {
  return workflow.on ?? workflow[true];
}

function runScripts(value) {
  if (Array.isArray(value)) return value.flatMap(runScripts);
  if (value && typeof value === "object") {
    return Object.entries(value).flatMap(([key, child]) =>
      key === "run" && typeof child === "string" ? [child] : runScripts(child),
    );
  }
  return [];
}

function conditions(value) {
  if (Array.isArray(value)) return value.flatMap(conditions);
  if (value && typeof value === "object") {
    return Object.entries(value).flatMap(([key, child]) =>
      key === "if" && typeof child === "string" ? [child] : conditions(child),
    );
  }
  return [];
}

function allStrings(value) {
  if (typeof value === "string") return [value];
  if (Array.isArray(value)) return value.flatMap(allStrings);
  if (value && typeof value === "object") {
    return Object.values(value).flatMap(allStrings);
  }
  return [];
}

test("Ait iOS TestFlight workflow is a guarded manual release", () => {
  const { parsed: workflow, source } = readWorkflow(workflowPath);
  const triggers = workflowTriggers(workflow);
  const jobs = Object.values(workflow.jobs ?? {});
  const scripts = runScripts(workflow);
  const scriptText = scripts.join("\n");

  assert.ok(triggers, "workflow must define triggers");
  assert.equal(triggers.push, undefined, "TestFlight release must not run on push");
  assert.doesNotMatch(source, /^\s+push:\s*$/m, "release must not declare a push trigger");
  assert.ok(triggers.workflow_dispatch, "release must be manual-only");
  assert.ok(
    conditions(workflow).some((condition) =>
      /github\.ref(?:_name)?\s*(?:==|===)\s*['"](?:refs\/heads\/)?main['"]/.test(condition),
    ),
    "release must be restricted to the main branch",
  );

  assert.deepEqual(workflow.permissions, { contents: "read" });
  assert.ok(
    jobs.some((job) => job.environment === "ios-testflight"),
    "a release job must use the ios-testflight environment",
  );
  assert.ok(
    jobs.some((job) =>
      allStrings(job).some((value) => value.includes("ios-testflight-release.mjs")),
    ),
    "release must invoke the checked-in TestFlight planning helper",
  );

  const secretNames = new Set(
    [...source.matchAll(/\$\{\{\s*secrets\.([A-Z0-9_]+)\s*\}\}/g)].map((match) => match[1]),
  );
  assert.deepEqual(secretNames, new Set(["EXPO_TOKEN"]));
  assert.doesNotMatch(
    scriptText,
    /\$\{\{\s*(?:inputs|github\.event\.inputs)\./,
    "run scripts must not interpolate workflow inputs",
  );

  assert.ok(
    allStrings(workflow).some((value) =>
      /(?:--profile\s+ait|profile\s*[:=]\s*["']?ait\b|EAS_PROFILE\s*=\s*ait)/.test(value),
    ),
    "build and submit commands must use the ait profile",
  );
  assert.match(scriptText, /--freeze-credentials/, "release must freeze EAS credentials");
  const identityStep = jobs
    .flatMap((job) => job.steps ?? [])
    .find((step) => step.name === "Load Ait EAS project identity");
  assert.ok(identityStep, "release must load its EAS identity before project queries");
  assert.match(identityStep.run, /build\.ait\.env/);
  assert.match(identityStep.run, /GITHUB_ENV/);
  for (const key of [
    "EXPO_OWNER",
    "EXPO_SLUG",
    "EAS_PROJECT_ID",
    "IOS_BUNDLE_IDENTIFIER",
    "APPLE_TEAM_ID",
  ]) {
    assert.match(identityStep.run, new RegExp(`['"]${key}['"]`));
  }
  const steps = jobs.flatMap((job) => job.steps ?? []);
  const identityIndex = steps.indexOf(identityStep);
  assert.ok(
    identityIndex < steps.findIndex((step) => step.name === "Check for duplicate EAS build"),
  );
  assert.ok(identityIndex < steps.findIndex((step) => step.name === "Select existing EAS build"));
  assert.ok(identityIndex < steps.findIndex((step) => step.name === "Submit build to TestFlight"));
  assert.match(
    scriptText,
    /(?:eas\s+build:list|build:list)[\s\S]*(?:duplicate|--status|--json)/i,
    "release must check for duplicate builds before creating one",
  );
  assert.match(
    scriptText,
    /(?:eas\s+submit|submit)[\s\S]*(?:--id|build_id)/i,
    "submission must use an explicit build ID",
  );
  assert.doesNotMatch(
    scriptText,
    /--auto-submit|--groups|fastlane|submit_review|submit_beta_review/,
  );
  assert.match(
    scriptText,
    /(?:verify:release|verify-release|app_version|version)/i,
    "release must verify the app version",
  );
  assert.match(
    scriptText,
    /(?:gh\s+release\s+view|github release|release\s+view)/i,
    "release must require an existing GitHub Release",
  );
  assert.match(
    scriptText,
    /GITHUB_STEP_SUMMARY/,
    "release must write its result to the step summary",
  );

  assert.equal(easConfig.build.ait.extends, "production");
  assert.equal(easConfig.build.ait.env.IOS_BUNDLE_IDENTIFIER, "com.necokeine.ait");
  assert.equal(easConfig.submit.ait.ios.bundleIdentifier, "com.necokeine.ait");
  assert.equal(easConfig.submit.ait.ios.appleTeamId, easConfig.build.ait.env.APPLE_TEAM_ID);
  assert.deepEqual(Object.keys(easConfig.submit.ait.ios).sort(), [
    "appleTeamId",
    "ascAppId",
    "bundleIdentifier",
  ]);
});

test("project identity reaches EAS steps without ambient Expo variables", () => {
  const { parsed: workflow } = readWorkflow(workflowPath);
  const step = workflow.jobs.testflight.steps.find(
    (candidate) => candidate.name === "Load Ait EAS project identity",
  );
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "ait-eas-identity-"));
  const output = path.join(directory, "github-env");
  const env = { ...process.env, GITHUB_ENV: output };
  const keys = [
    "EXPO_OWNER",
    "EXPO_SLUG",
    "EAS_PROJECT_ID",
    "IOS_BUNDLE_IDENTIFIER",
    "APPLE_TEAM_ID",
  ];
  for (const key of keys) delete env[key];

  try {
    const result = spawnSync("bash", ["-eo", "pipefail", "-c", step.run], {
      cwd: rootDir,
      env,
      encoding: "utf8",
    });
    assert.equal(result.status, 0, result.stderr);
    const exported = Object.fromEntries(
      fs
        .readFileSync(output, "utf8")
        .trim()
        .split("\n")
        .map((line) => line.split("=")),
    );
    for (const key of keys) assert.equal(exported[key], easConfig.build.ait.env[key]);
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
});

for (const workflowName of ["release-mobile.yml", "release-ios-beta.yml"]) {
  test(`${workflowName} remains manually dispatched only`, () => {
    const workflowPath = `${appDir}/.eas/workflows/${workflowName}`;
    const { parsed: workflow, source } = readWorkflow(workflowPath);
    const triggers = workflowTriggers(workflow);

    assert.equal(triggers.push, undefined, `${workflowName} must not run on push`);
    assert.doesNotMatch(
      source,
      /^\s+push:\s*$/m,
      `${workflowName} must not declare a push trigger`,
    );
    assert.deepEqual(
      triggers.workflow_dispatch,
      {},
      `${workflowName} must retain workflow_dispatch`,
    );
  });
}
