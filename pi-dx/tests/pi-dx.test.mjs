import assert from "node:assert/strict";
import { chmod, mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { delimiter, dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { discoverAndLoadExtensions } from "@earendil-works/pi-coding-agent";
import extension, { dxInvocationNeedsGit, parseCommandLine } from "../extensions/pi-dx.ts";

const packageRoot = join(dirname(fileURLToPath(import.meta.url)), "..");

test("extension registers dx source commands", () => {
  const registered = [];
  extension({
    registerCommand(name, options) {
      registered.push({ name, description: options.description });
    },
  });

  assert.deepEqual(registered, [
    { name: "diff", description: "Open the current diff in dx" },
    { name: "show", description: "Open a revision or hosted review in dx" },
    { name: "patch", description: "Open a patch file in dx" },
  ]);
});

test("package manifest loads dx source commands", async () => {
  const agentDir = await mkdtemp(join(tmpdir(), "pi-dx-test-"));

  try {
    const result = await discoverAndLoadExtensions([packageRoot], packageRoot, agentDir);
    assert.deepEqual(result.errors, []);

    const loaded = result.extensions.find(
      (loadedExtension) =>
        loadedExtension.commands.has("diff") &&
        loadedExtension.commands.has("show") &&
        loadedExtension.commands.has("patch"),
    );
    assert.ok(loaded, "expected package manifest to load dx source commands");
  } finally {
    await rm(agentDir, { recursive: true, force: true });
  }
});

test("parseCommandLine splits whitespace", () => {
  assert.deepEqual(parseCommandLine("--staged --base main"), ["--staged", "--base", "main"]);
});

test("parseCommandLine preserves quoted arguments", () => {
  assert.deepEqual(parseCommandLine('review "changes with spaces"'), [
    "review",
    "changes with spaces",
  ]);
});

test("parseCommandLine rejects unterminated quotes", () => {
  assert.throws(() => parseCommandLine('review "missing'), /Unterminated double quote/);
});

test("dxInvocationNeedsGit allows patch files", () => {
  assert.equal(dxInvocationNeedsGit("patch", ["changes.diff"]), false);
  assert.equal(dxInvocationNeedsGit("patch", ["--stat", "changes.diff"]), false);
});

test("dxInvocationNeedsGit allows full GitHub pull request URLs", () => {
  assert.equal(
    dxInvocationNeedsGit("show", ["review", "https://github.com/owner/repo/pull/123"]),
    false,
  );
  assert.equal(
    dxInvocationNeedsGit("show", ["review", "https://github.com/owner/repo/pull/123/"]),
    false,
  );
  assert.equal(
    dxInvocationNeedsGit("show", ["review", "https://github.com/owner/repo/pull/123/files"]),
    false,
  );
  assert.equal(
    dxInvocationNeedsGit("show", [
      "review",
      "https://github.com/owner/repo/pull/123/files?diff=split",
    ]),
    false,
  );
});

test("dxInvocationNeedsGit requires git for diffs, revisions, and review numbers", () => {
  assert.equal(dxInvocationNeedsGit("diff", []), true);
  assert.equal(dxInvocationNeedsGit("diff", ["--staged"]), true);
  assert.equal(dxInvocationNeedsGit("show", []), true);
  assert.equal(dxInvocationNeedsGit("show", ["HEAD~1"]), true);
  assert.equal(dxInvocationNeedsGit("show", ["review", "123"]), true);
});

test("diff command preflight honors attached short repo arguments without waiting for idle", async () => {
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
        registerCommand(name, options) {
          if (name === "diff") {
            handler = options.handler;
          }
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
          async custom(render) {
            customCalled = true;
            let result;
            await render(
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

      assert.equal(waitForIdleCalled, false, `expected ${args} to open without waiting for idle`);
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

test("diff command uses filesystem git marker fast path", async () => {
  const tempDir = await mkdtemp(join(tmpdir(), "pi-dx-test-"));
  const binDir = join(tempDir, "bin");
  const repoDir = join(tempDir, "repo");
  const outsideDir = join(tempDir, "outside");
  const dxPath = join(binDir, "dx");
  const gitPath = join(binDir, "git");
  const oldPiDxBin = process.env.PI_DX_BIN;
  const oldPath = process.env.PATH;

  try {
    await mkdir(binDir);
    await mkdir(repoDir);
    await mkdir(join(repoDir, ".git"));
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
process.exit(1);
`,
    );
    await chmod(dxPath, 0o755);
    await chmod(gitPath, 0o755);

    process.env.PI_DX_BIN = "dx";
    process.env.PATH = `${binDir}${delimiter}${oldPath ?? ""}`;

    const notifications = [];
    let customCalled = false;
    let handler;

    extension({
      registerCommand(name, options) {
        if (name === "diff") {
          handler = options.handler;
        }
      },
    });

    await handler(`--repo=${repoDir}`, {
      mode: "tui",
      cwd: outsideDir,
      hasUI: true,
      ui: {
        notify(message, level) {
          notifications.push({ message, level });
        },
        async custom(render) {
          customCalled = true;
          let result;
          await render(
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
        throw new Error("waitForIdle should not be called");
      },
    });

    assert.equal(customCalled, true, "expected /diff to run dx");
    assert.deepEqual(notifications, []);
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
    await rm(tempDir, { recursive: true, force: true });
  }
});
