import assert from "node:assert/strict";
import { chmod, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { delimiter, dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { discoverAndLoadExtensions } from "@earendil-works/pi-coding-agent";
import extension, { markInvocationNeedsGit, parseCommandLine } from "../extensions/pi-mark.ts";

const packageRoot = join(dirname(fileURLToPath(import.meta.url)), "..");

test("extension registers mark source commands", () => {
  const registered = [];
  extension({
    registerCommand(name, options) {
      registered.push({ name, description: options.description });
    },
  });

  assert.deepEqual(registered, [{ name: "mark", description: "Open mark diff reviewer" }]);
});

test("package manifest loads mark source commands", async () => {
  const agentDir = await mkdtemp(join(tmpdir(), "pi-mark-test-"));

  try {
    const result = await discoverAndLoadExtensions([packageRoot], packageRoot, agentDir);
    assert.deepEqual(result.errors, []);

    const loaded = result.extensions.find((loadedExtension) =>
      loadedExtension.commands.has("mark"),
    );
    assert.ok(loaded, "expected package manifest to load mark source commands");
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

test("markInvocationNeedsGit allows patch files", () => {
  assert.equal(markInvocationNeedsGit("patch", ["changes.diff"]), false);
  assert.equal(markInvocationNeedsGit("patch", ["--stat", "changes.diff"]), false);
  assert.equal(markInvocationNeedsGit("diff", ["--patch", "changes.diff"]), false);
  assert.equal(markInvocationNeedsGit("diff", ["--patch=changes.diff"]), false);
});

test("markInvocationNeedsGit allows full GitHub pull request URLs", () => {
  assert.equal(
    markInvocationNeedsGit("show", ["review", "https://github.com/owner/repo/pull/123"]),
    false,
  );
  assert.equal(
    markInvocationNeedsGit("show", ["review", "https://github.com/owner/repo/pull/123/"]),
    false,
  );
  assert.equal(
    markInvocationNeedsGit("show", ["review", "https://github.com/owner/repo/pull/123/files"]),
    false,
  );
  assert.equal(
    markInvocationNeedsGit("show", [
      "review",
      "https://github.com/owner/repo/pull/123/files?diff=split",
    ]),
    false,
  );
  assert.equal(
    markInvocationNeedsGit("show", ["review", "--stat", "https://github.com/owner/repo/pull/123"]),
    false,
  );
  assert.equal(
    markInvocationNeedsGit("show", [
      "review",
      "--repo",
      "/tmp/not-a-repo",
      "https://github.com/owner/repo/pull/123",
    ]),
    false,
  );
  assert.equal(
    markInvocationNeedsGit("diff", ["--pr", "https://github.com/owner/repo/pull/123"]),
    false,
  );
  assert.equal(
    markInvocationNeedsGit("diff", ["--pr=https://github.com/owner/repo/pull/123"]),
    false,
  );
});

test("markInvocationNeedsGit requires git for diffs, revisions, and review numbers", () => {
  assert.equal(markInvocationNeedsGit("diff", []), true);
  assert.equal(markInvocationNeedsGit("diff", ["--staged"]), true);
  assert.equal(markInvocationNeedsGit("diff", ["--pr", "123"]), true);
  assert.equal(markInvocationNeedsGit("show", []), true);
  assert.equal(markInvocationNeedsGit("show", ["HEAD~1"]), true);
  assert.equal(markInvocationNeedsGit("show", ["review", "123"]), true);
});

test("mark command rejects stdin patch sources before preflight", async () => {
  let handler;
  extension({
    registerCommand(name, options) {
      if (name === "mark") {
        handler = options.handler;
      }
    },
  });

  const notifications = [];
  let customCalled = false;

  await handler("--patch -", {
    mode: "tui",
    cwd: packageRoot,
    hasUI: true,
    ui: {
      notify(message, level) {
        notifications.push({ message, level });
      },
      async custom() {
        customCalled = true;
      },
    },
    async waitForIdle() {
      throw new Error("waitForIdle should not be called");
    },
  });

  assert.equal(customCalled, false);
  assert.deepEqual(notifications, [
    {
      message:
        "/mark --patch cannot read a patch from stdin inside Pi. Write the patch to a file and run /mark patch <file>.",
      level: "error",
    },
  ]);
});

test("top-level help and version flags run mark without git preflight", async () => {
  const tempDir = await mkdtemp(join(tmpdir(), "pi-mark-test-"));
  const markPath = join(tempDir, "mark");
  const argsPath = join(tempDir, "args.json");
  const oldPiMarkBin = process.env.PI_MARK_BIN;
  const oldArgsPath = process.env.PI_MARK_TEST_ARGS;

  try {
    await writeFile(
      markPath,
      `#!/usr/bin/env node
import { writeFileSync } from "node:fs";
writeFileSync(process.env.PI_MARK_TEST_ARGS, JSON.stringify(process.argv.slice(2)));
process.exit(0);
`,
    );
    await chmod(markPath, 0o755);

    process.env.PI_MARK_BIN = markPath;
    process.env.PI_MARK_TEST_ARGS = argsPath;

    const handlers = new Map();
    extension({
      registerCommand(name, options) {
        handlers.set(name, options.handler);
      },
    });

    for (const [args, expected] of [
      ["help", ["help"]],
      ["help diff", ["help", "diff"]],
      ["--version", ["--version"]],
      ["-V", ["-V"]],
      ["diff --version", ["--version"]],
      ["show -V", ["-V"]],
      ["patch --version", ["--version"]],
    ]) {
      const notifications = [];
      let customCalled = false;
      await writeFile(argsPath, "[]");

      await handlers.get("mark")(args, {
        mode: "tui",
        cwd: tempDir,
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

      assert.equal(customCalled, true, `expected /mark ${args} to run mark`);
      assert.deepEqual(notifications, []);
      assert.deepEqual(JSON.parse(await readFile(argsPath, "utf8")), expected);
    }
  } finally {
    if (oldPiMarkBin === undefined) {
      delete process.env.PI_MARK_BIN;
    } else {
      process.env.PI_MARK_BIN = oldPiMarkBin;
    }
    if (oldArgsPath === undefined) {
      delete process.env.PI_MARK_TEST_ARGS;
    } else {
      process.env.PI_MARK_TEST_ARGS = oldArgsPath;
    }
    await rm(tempDir, { recursive: true, force: true });
  }
});

test("mark command forwards default diff and explicit subcommands", async () => {
  const tempDir = await mkdtemp(join(tmpdir(), "pi-mark-test-"));
  const repoDir = join(tempDir, "repo");
  const markPath = join(tempDir, "mark");
  const argsPath = join(tempDir, "args.json");
  const oldPiMarkBin = process.env.PI_MARK_BIN;
  const oldArgsPath = process.env.PI_MARK_TEST_ARGS;

  try {
    await mkdir(repoDir);
    await mkdir(join(repoDir, ".git"));
    await writeFile(
      markPath,
      `#!/usr/bin/env node
import { writeFileSync } from "node:fs";
writeFileSync(process.env.PI_MARK_TEST_ARGS, JSON.stringify(process.argv.slice(2)));
process.exit(0);
`,
    );
    await chmod(markPath, 0o755);

    process.env.PI_MARK_BIN = markPath;
    process.env.PI_MARK_TEST_ARGS = argsPath;

    let handler;
    extension({
      registerCommand(name, options) {
        if (name === "mark") {
          handler = options.handler;
        }
      },
    });

    for (const [args, expected] of [
      ["", []],
      ["--staged", ["--staged"]],
      ["diff --staged", ["diff", "--staged"]],
      ["show HEAD~1", ["show", "HEAD~1"]],
      ["patch changes.diff", ["patch", "changes.diff"]],
    ]) {
      const notifications = [];
      await writeFile(argsPath, "[]");

      await handler(args, {
        mode: "tui",
        cwd: repoDir,
        hasUI: true,
        ui: {
          notify(message, level) {
            notifications.push({ message, level });
          },
          async custom(render) {
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
      });

      assert.deepEqual(notifications, []);
      assert.deepEqual(JSON.parse(await readFile(argsPath, "utf8")), expected);
    }
  } finally {
    if (oldPiMarkBin === undefined) {
      delete process.env.PI_MARK_BIN;
    } else {
      process.env.PI_MARK_BIN = oldPiMarkBin;
    }
    if (oldArgsPath === undefined) {
      delete process.env.PI_MARK_TEST_ARGS;
    } else {
      process.env.PI_MARK_TEST_ARGS = oldArgsPath;
    }
    await rm(tempDir, { recursive: true, force: true });
  }
});

test("mark command preflight honors attached short repo arguments without waiting for idle", async () => {
  const tempDir = await mkdtemp(join(tmpdir(), "pi-mark-test-"));
  const binDir = join(tempDir, "bin");
  const repoDir = join(tempDir, "repo");
  const outsideDir = join(tempDir, "outside");
  const markPath = join(binDir, "mark");
  const gitPath = join(binDir, "git");
  const oldPiMarkBin = process.env.PI_MARK_BIN;
  const oldPath = process.env.PATH;
  const oldExpectedRepo = process.env.PI_MARK_TEST_EXPECTED_REPO;

  try {
    await mkdir(binDir);
    await mkdir(repoDir);
    await mkdir(outsideDir);
    await writeFile(
      markPath,
      `#!/usr/bin/env node
process.exit(0);
`,
    );
    await writeFile(
      gitPath,
      `#!/usr/bin/env node
const args = process.argv.slice(2);
const expectedRepo = process.env.PI_MARK_TEST_EXPECTED_REPO;
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
    await chmod(markPath, 0o755);
    await chmod(gitPath, 0o755);

    process.env.PI_MARK_BIN = markPath;
    process.env.PATH = `${binDir}${delimiter}${oldPath ?? ""}`;

    for (const { args, expectedRepo } of [
      { args: "-r../repo", expectedRepo: "../repo" },
      { args: `-r=${repoDir}`, expectedRepo: repoDir },
    ]) {
      process.env.PI_MARK_TEST_EXPECTED_REPO = expectedRepo;
      const notifications = [];
      let customCalled = false;
      let waitForIdleCalled = false;
      let handler;

      extension({
        registerCommand(name, options) {
          if (name === "mark") {
            handler = options.handler;
          }
        },
      });

      await handler(`diff ${args}`, {
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

      assert.equal(
        waitForIdleCalled,
        false,
        `expected /mark diff ${args} to open without waiting for idle`,
      );
      assert.equal(customCalled, true, `expected ${args} to run mark`);
      assert.deepEqual(notifications, []);
    }
  } finally {
    if (oldPiMarkBin === undefined) {
      delete process.env.PI_MARK_BIN;
    } else {
      process.env.PI_MARK_BIN = oldPiMarkBin;
    }
    if (oldPath === undefined) {
      delete process.env.PATH;
    } else {
      process.env.PATH = oldPath;
    }
    if (oldExpectedRepo === undefined) {
      delete process.env.PI_MARK_TEST_EXPECTED_REPO;
    } else {
      process.env.PI_MARK_TEST_EXPECTED_REPO = oldExpectedRepo;
    }
    await rm(tempDir, { recursive: true, force: true });
  }
});

test("mark command uses filesystem git marker fast path", async () => {
  const tempDir = await mkdtemp(join(tmpdir(), "pi-mark-test-"));
  const binDir = join(tempDir, "bin");
  const repoDir = join(tempDir, "repo");
  const outsideDir = join(tempDir, "outside");
  const markPath = join(binDir, "mark");
  const gitPath = join(binDir, "git");
  const oldPiMarkBin = process.env.PI_MARK_BIN;
  const oldPath = process.env.PATH;

  try {
    await mkdir(binDir);
    await mkdir(repoDir);
    await mkdir(join(repoDir, ".git"));
    await mkdir(outsideDir);
    await writeFile(
      markPath,
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
    await chmod(markPath, 0o755);
    await chmod(gitPath, 0o755);

    process.env.PI_MARK_BIN = "mark";
    process.env.PATH = `${binDir}${delimiter}${oldPath ?? ""}`;

    const notifications = [];
    let customCalled = false;
    let handler;

    extension({
      registerCommand(name, options) {
        if (name === "mark") {
          handler = options.handler;
        }
      },
    });

    await handler(`diff --repo=${repoDir}`, {
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

    assert.equal(customCalled, true, "expected /mark diff to run mark");
    assert.deepEqual(notifications, []);
  } finally {
    if (oldPiMarkBin === undefined) {
      delete process.env.PI_MARK_BIN;
    } else {
      process.env.PI_MARK_BIN = oldPiMarkBin;
    }
    if (oldPath === undefined) {
      delete process.env.PATH;
    } else {
      process.env.PATH = oldPath;
    }
    await rm(tempDir, { recursive: true, force: true });
  }
});
