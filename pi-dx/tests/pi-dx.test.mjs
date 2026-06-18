import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { discoverAndLoadExtensions } from "@earendil-works/pi-coding-agent";
import extension, { dxInvocationNeedsGit, parseCommandLine } from "../extensions/pi-dx.ts";

const packageRoot = join(dirname(fileURLToPath(import.meta.url)), "..");

test("extension registers /diff", () => {
  let registered;
  extension({
    registerCommand(name, options) {
      registered = { name, description: options.description };
    },
  });

  assert.deepEqual(registered, { name: "diff", description: "Open the current diff in dx" });
});

test("package manifest loads /diff extension", async () => {
  const agentDir = await mkdtemp(join(tmpdir(), "pi-dx-test-"));

  try {
    const result = await discoverAndLoadExtensions([packageRoot], packageRoot, agentDir);
    assert.deepEqual(result.errors, []);

    const diffExtension = result.extensions.find((loadedExtension) =>
      loadedExtension.commands.has("diff"),
    );
    assert.ok(diffExtension, "expected package manifest to load the /diff extension");
  } finally {
    await rm(agentDir, { recursive: true, force: true });
  }
});

test("parseCommandLine splits whitespace", () => {
  assert.deepEqual(parseCommandLine("--staged --base main"), ["--staged", "--base", "main"]);
});

test("parseCommandLine preserves quoted arguments", () => {
  assert.deepEqual(parseCommandLine('--patch "changes with spaces.diff"'), [
    "--patch",
    "changes with spaces.diff",
  ]);
});

test("parseCommandLine rejects unterminated quotes", () => {
  assert.throws(() => parseCommandLine('--patch "missing'), /Unterminated double quote/);
});

test("dxInvocationNeedsGit allows patch files", () => {
  assert.equal(dxInvocationNeedsGit(["--patch", "changes.diff"]), false);
  assert.equal(dxInvocationNeedsGit(["--patch=changes.diff"]), false);
});

test("dxInvocationNeedsGit allows full GitHub pull request URLs", () => {
  assert.equal(dxInvocationNeedsGit(["--pr", "https://github.com/owner/repo/pull/123"]), false);
  assert.equal(dxInvocationNeedsGit(["--pr", "https://github.com/owner/repo/pull/123/"]), false);
  assert.equal(
    dxInvocationNeedsGit(["--pr", "https://github.com/owner/repo/pull/123/files"]),
    false,
  );
  assert.equal(dxInvocationNeedsGit(["--pr=github.com/owner/repo/pull/123/files"]), false);
  assert.equal(
    dxInvocationNeedsGit(["--pr", "https://github.com/owner/repo/pull/123/files?diff=split"]),
    false,
  );
});

test("dxInvocationNeedsGit requires git for regular diffs and pull request numbers", () => {
  assert.equal(dxInvocationNeedsGit([]), true);
  assert.equal(dxInvocationNeedsGit(["--staged"]), true);
  assert.equal(dxInvocationNeedsGit(["--pr", "123"]), true);
});
