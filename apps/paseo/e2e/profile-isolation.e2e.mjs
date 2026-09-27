import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { _electron as electron } from "playwright";

const require = createRequire(import.meta.url);
const desktop = fileURLToPath(new URL("..", import.meta.url));
const temporary = mkdtempSync(path.join(tmpdir(), "ait-profile-isolation-"));
const fixture = path.join(temporary, "fixture.cjs");
const appData = path.join(temporary, "app-data");
mkdirSync(appData);
writeFileSync(
  fixture,
  `const { app, BrowserWindow, protocol } = require("electron");
const { configureDesktopProfile } = require(${JSON.stringify(path.join(desktop, "dist/branding.js"))});
app.setPath("appData", ${JSON.stringify(appData)});
const legacy = process.env.ISOLATION_PROFILE === "Paseo";
if (legacy) {
  app.setName("Paseo");
  app.setPath("userData", ${JSON.stringify(path.join(appData, "Paseo"))});
  app.setPath("sessionData", app.getPath("userData"));
} else {
  configureDesktopProfile(app);
}
protocol.registerSchemesAsPrivileged([
  { scheme: "ait", privileges: { standard: true, secure: true } },
]);
if (!app.requestSingleInstanceLock()) app.exit(2);
app.whenReady().then(async () => {
  protocol.handle("ait", () => new Response("<!doctype html><title>Isolation fixture</title>"));
  const win = new BrowserWindow({ show: false });
  // Use the same origin in both processes to test directory isolation itself.
  await win.loadURL("ait://app/");
});
`,
);

const env = { ...process.env };
delete env.ELECTRON_RUN_AS_NODE;
const apps = [];
async function launch(profile) {
  const instance = await electron.launch({
    executablePath: require("electron"),
    args: [fixture],
    env: { ...env, ISOLATION_PROFILE: profile },
    timeout: 30_000,
  });
  apps.push(instance);
  const page = await instance.firstWindow();
  await page.waitForURL("ait://app/");
  return { instance, page };
}

try {
  mkdirSync(path.join(appData, "Paseo"));
  const paseoSettings = path.join(appData, "Paseo", "desktop-settings.json");
  writeFileSync(paseoSettings, JSON.stringify({ owner: "Paseo" }));
  const paseo = await launch("Paseo");
  await paseo.page.evaluate(() => localStorage.setItem("@paseo:daemon-registry", "paseo hosts"));
  await paseo.instance.evaluate(async ({ session }) => {
    await session.defaultSession.cookies.set({
      url: "https://isolation.invalid",
      name: "login",
      value: "paseo-login",
    });
  });

  const ait = await launch("Ait");
  assert.deepEqual(
    await ait.instance.evaluate(({ app }) => [app.getPath("userData"), app.getPath("sessionData")]),
    [path.join(appData, "Ait"), path.join(appData, "Ait")],
  );
  assert.equal(await ait.page.evaluate(() => localStorage.getItem("@paseo:daemon-registry")), null);
  assert.deepEqual(
    await ait.instance.evaluate(({ session }) =>
      session.defaultSession.cookies.get({ name: "login" }),
    ),
    [],
  );
  await ait.page.evaluate(() => localStorage.setItem("@paseo:daemon-registry", "ait hosts"));
  await ait.instance.evaluate(async ({ session }) => {
    await session.defaultSession.cookies.set({
      url: "https://isolation.invalid",
      name: "login",
      value: "ait-login",
    });
  });
  assert.equal(
    await paseo.page.evaluate(() => localStorage.getItem("@paseo:daemon-registry")),
    "paseo hosts",
  );
  const cookies = await paseo.instance.evaluate(({ session }) =>
    session.defaultSession.cookies.get({ name: "login" }),
  );
  assert.equal(cookies[0].value, "paseo-login");
  assert.deepEqual(JSON.parse(readFileSync(paseoSettings, "utf8")), { owner: "Paseo" });
  await ait.instance.close();
  apps.splice(apps.indexOf(ait.instance), 1);
  const reopened = await launch("Ait");
  assert.equal(
    await reopened.page.evaluate(() => localStorage.getItem("@paseo:daemon-registry")),
    "ait hosts",
  );
  console.log(
    "PASS: concurrent profiles, cookies, local storage, restart persistence, untouched Paseo settings",
  );
} finally {
  for (const instance of apps.reverse()) await instance.close();
  rmSync(temporary, { recursive: true, force: true });
}
