import assert from "node:assert/strict";
import { chmod, mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { delimiter, dirname, join } from "node:path";
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

test("diff command preflight honors attached short repo arguments", async () => {
  const tempDir = await mkdtemp(join(tmpdir(), "pi-dx-test-"));
  const binDir = join(tempDir, "bin");
  const repoDir = join(tempDir, "repo");
  const outsideDir = join(tempDir, "outside");
  const dxPath = join(binDir, "dx");
  const gitPath = join(binDir, "git");
  const oldPiDxBin = process.env.PI_DX_BIN;
  const oldPath = process.env.PATH;
  const oldExpectedRepo = process.env.PI_DX_TEST_EXPECTED_REPO;

  try {
    await mkdir(binDir);
    await mkdir(repoDir);
    await mkdir(outsideDir);
    await writeFile(
      dxPath,
      `#!/usr/bin/env node
process.exit(0);
`,
    );
    await writeFile(
      gitPath,
      `#!/usr/bin/env node
const args = process.argv.slice(2);
const expectedRepo = process.env.PI_DX_TEST_EXPECTED_REPO;
if (
  expectedRepo &&
  args.length === 4 &&
  args[0] === "-C" &&
  args[1] === expectedRepo &&
  args[2] === "rev-parse" &&
  args[3] === "--is-inside-work-tree"
) {
  console.log("true");
  process.exit(0);
}
process.exit(1);
`,
    );
    await chmod(dxPath, 0o755);
    await chmod(gitPath, 0o755);

    process.env.PI_DX_BIN = dxPath;
    process.env.PATH = `${binDir}${delimiter}${oldPath ?? ""}`;

    for (const { args, expectedRepo } of [
      { args: "-r../repo", expectedRepo: "../repo" },
      { args: `-r=${repoDir}`, expectedRepo: repoDir },
    ]) {
      process.env.PI_DX_TEST_EXPECTED_REPO = expectedRepo;
      const notifications = [];
      let customCalled = false;
      let waitForIdleCalled = false;
      let handler;

      extension({
        registerCommand(_name, options) {
          handler = options.handler;
        },
      });

      await handler(args, {
        mode: "tui",
        cwd: outsideDir,
        hasUI: true,
        ui: {
          notify(message, level) {
            notifications.push({ message, level });
          },
          custom(render) {
            customCalled = true;
            let result;
            render(
              {
                stop() {},
                start() {},
                requestRender() {},
              },
              undefined,
              undefined,
              (value) => {
                result = value;
              },
            );
            return result;
          },
        },
        async waitForIdle() {
          waitForIdleCalled = true;
        },
      });

      assert.equal(waitForIdleCalled, true, `expected ${args} to pass Git preflight`);
      assert.equal(customCalled, true, `expected ${args} to run dx`);
      assert.deepEqual(notifications, []);
    }
  } finally {
    if (oldPiDxBin === undefined) {
      delete process.env.PI_DX_BIN;
    } else {
      process.env.PI_DX_BIN = oldPiDxBin;
    }
    if (oldPath === undefined) {
      delete process.env.PATH;
    } else {
      process.env.PATH = oldPath;
    }
    if (oldExpectedRepo === undefined) {
      delete process.env.PI_DX_TEST_EXPECTED_REPO;
    } else {
      process.env.PI_DX_TEST_EXPECTED_REPO = oldExpectedRepo;
    }
    await rm(tempDir, { recursive: true, force: true });
  }
});
