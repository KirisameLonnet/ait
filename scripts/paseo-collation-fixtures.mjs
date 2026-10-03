// Capture the actual upstream pager comparator in separate ICU process locales.
// Run: node scripts/paseo-collation-fixtures.mjs /path/to/paseo [--check]
import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { execFileSync } from "node:child_process";

const root = path.resolve(process.argv[2] ?? "../paseo");
const require = createRequire(path.join(root, "package.json"));
const sourceFile = "packages/server/src/server/pagination/sortable-pager.ts";
const values = [
  "a",
  "A",
  "á",
  "ä",
  "b",
  "é",
  "e\u0301",
  "E",
  "z",
  "å",
  "ö",
  "a-b",
  "a_b",
  "a b",
  "10",
  "2",
  "中文",
  "北京",
  "上海",
  "广州",
  "工作区",
  "文件",
  "项目",
  "重构",
  "重庆",
  "東京",
  "大阪",
  "가",
  "나",
  "α",
  "ω",
  "😀",
  "🦀",
];
if (process.argv.includes("--child")) {
  const { tsImport } = require("tsx/esm/api");
  const { compareValues } = await tsImport(path.join(root, sourceFile), import.meta.url);
  console.log(
    JSON.stringify({
      environment: process.env.LC_ALL,
      locale: Intl.Collator().resolvedOptions().locale,
      expected: [...values].sort(compareValues),
    }),
  );
} else {
  const revision = execFileSync("git", ["-C", root, "rev-parse", "HEAD"], {
    encoding: "utf8",
  }).trim();
  assert.equal(revision, "30178c4f58b67f8472901356e1484022bd835de0");
  const cases = ["en_US.UTF-8", "zh_CN.UTF-8", "sv_SE.UTF-8"].map((locale) =>
    JSON.parse(
      execFileSync(process.execPath, [process.argv[1], root, "--child"], {
        encoding: "utf8",
        env: { ...process.env, LC_ALL: locale },
      }),
    ),
  );
  const fixture = {
    source: {
      revision,
      sourceFile,
      sha256: crypto
        .createHash("sha256")
        .update(fs.readFileSync(path.join(root, sourceFile)))
        .digest("hex"),
      node: process.version,
      icu: process.versions.icu,
    },
    values,
    cases,
  };
  const destination = "crates/model/tests/fixtures/paseo-collation.json";
  const content = `${JSON.stringify(fixture, null, 2)}\n`;
  if (process.argv.includes("--check")) {
    assert.equal(fs.readFileSync(destination, "utf8"), content, `${destination} drifted`);
  } else {
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    fs.writeFileSync(destination, content);
  }
  console.log(JSON.stringify({ destination, locales: cases.length, values: values.length }));
}
