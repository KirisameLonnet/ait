const assert = require("node:assert/strict");
const path = require("node:path");
const test = require("node:test");
const { getConfig } = require("@expo/config");

test("Expo production and development identities coexist with Paseo", () => {
  const saved = { ...process.env };
  try {
    delete process.env.IOS_BUNDLE_IDENTIFIER;
    for (const [variant, id] of [
      ["production", "dev.ait.mobile"],
      ["development", "dev.ait.mobile.debug"],
    ]) {
      process.env.APP_VARIANT = variant;
      const { exp } = getConfig(path.resolve(__dirname, ".."), { skipPlugins: true });
      assert.equal(exp.ios.bundleIdentifier, id);
      assert.equal(exp.android.package, id);
      assert.equal(exp.scheme, "ait");
    }
  } finally {
    for (const key of ["APP_VARIANT", "IOS_BUNDLE_IDENTIFIER"]) {
      if (saved[key] === undefined) delete process.env[key];
      else process.env[key] = saved[key];
    }
  }
});
