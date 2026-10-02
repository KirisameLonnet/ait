import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { checkDocumentationLinks } from "./check-docs.mjs";

test("checks local links and images without interpreting examples or remote URLs", (t) => {
  const root = mkdtempSync(path.join(tmpdir(), "ait-doc-links-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(path.join(root, "guide"));
  writeFileSync(path.join(root, "guide", "present file.md"), "# Present");
  const file = path.join(root, "README.md");
  writeFileSync(
    file,
    [
      "[Present](guide/present%20file.md#present)",
      "[Directory](guide)",
      "[External](https://example.com/missing.md)",
      "[Anchor](#heading)",
      "```markdown\n[Example](missing-example.md)\n```",
      "[Missing](missing.md)",
      "![Missing image](missing.png)",
    ].join("\n"),
  );
  assert.deepEqual(checkDocumentationLinks([file]), [
    `${file}: missing local link missing.md`,
    `${file}: missing local link missing.png`,
  ]);
});
