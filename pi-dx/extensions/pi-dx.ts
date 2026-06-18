import { spawnSync } from "node:child_process";
import { accessSync, constants } from "node:fs";
import type { ExtensionAPI, ExtensionCommandContext } from "@earendil-works/pi-coding-agent";

const INSTALL_HINT =
  "Install dx with:\n" +
  "curl -fsSL https://raw.githubusercontent.com/phongndo/dx/main/scripts/install.sh | sh";

type NotifyLevel = "info" | "warning" | "error";

type DxRunResult = {
  status: number | null;
  signal: string | null;
  error?: string;
};

export default function piDx(pi: ExtensionAPI) {
  pi.registerCommand("diff", {
    description: "Open the current diff in dx",
    handler: async (args, ctx) => {
      await handleDiffCommand(args, ctx);
    },
  });
}

async function handleDiffCommand(args: string, ctx: ExtensionCommandContext): Promise<void> {
  if (ctx.mode !== "tui") {
    report(ctx, "/diff requires Pi interactive TUI mode.", "error");
    return;
  }

  let argv: string[];
  try {
    argv = parseCommandLine(args);
  } catch (error) {
    report(ctx, errorMessage(error), "error");
    return;
  }

  if (stdinPatchRequested(argv)) {
    report(
      ctx,
      "/diff cannot read a patch from stdin inside Pi. Write the patch to a file and run /diff --patch <file>.",
      "error",
    );
    return;
  }

  const dx = dxBinary();
  const dxError = checkDxBinary(dx);
  if (dxError) {
    report(ctx, dxError, "error");
    return;
  }

  if (dxInvocationNeedsGit(argv)) {
    const repoPath = repoPathFromArgs(argv);
    if (repoPath === null) {
      report(ctx, "/diff --repo requires a repository path.", "error");
      return;
    }

    const gitError = checkGitRepository(ctx.cwd, repoPath);
    if (gitError) {
      report(ctx, gitError, "error");
      return;
    }
  }

  await ctx.waitForIdle();

  const result = await runDxInTerminal(ctx, dx, argv);
  if (!result) {
    report(ctx, "dx did not return a result.", "error");
    return;
  }

  if (result.error) {
    report(ctx, `Failed to run dx: ${result.error}`, "error");
    return;
  }

  if (result.signal) {
    report(ctx, `dx terminated by signal ${result.signal}.`, "warning");
    return;
  }

  if (result.status !== 0) {
    report(ctx, `dx exited with status ${result.status}.`, "error");
  }
}

function dxBinary(): string {
  return process.env.PI_DX_BIN?.trim() || "dx";
}

function checkDxBinary(dx: string): string | undefined {
  if (!dx) {
    return `PI_DX_BIN is empty.\n\n${INSTALL_HINT}`;
  }

  if (looksLikePath(dx)) {
    try {
      accessSync(dx, constants.X_OK);
    } catch (error) {
      return `dx executable is not available at PI_DX_BIN=${dx}: ${errorMessage(error)}\n\n${INSTALL_HINT}`;
    }
  }

  const result = spawnSync(dx, ["--version"], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });

  if (result.error) {
    return `dx executable was not found (${dx}).\n\n${INSTALL_HINT}`;
  }

  if (result.status !== 0) {
    const stderr = result.stderr.trim();
    const detail = stderr ? `\n\n${stderr}` : "";
    return `dx was found but failed to start with status ${result.status}.${detail}`;
  }
}

function looksLikePath(command: string): boolean {
  return command.includes("/") || command.includes("\\");
}

function checkGitRepository(cwd: string, repoPath: string | undefined): string | undefined {
  const gitArgs = repoPath
    ? ["-C", repoPath, "rev-parse", "--is-inside-work-tree"]
    : ["rev-parse", "--is-inside-work-tree"];
  const result = spawnSync("git", gitArgs, {
    cwd,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });

  if (result.error) {
    return `git is required for the default /diff mode but was not found: ${errorMessage(result.error)}`;
  }

  if (result.status !== 0 || result.stdout.trim() !== "true") {
    const target = repoPath ? `repository path ${repoPath}` : cwd;
    return (
      `No Git repository found at ${target}.\n\n` +
      "/diff currently opens Git-backed dx diffs unless you pass --patch <file> or a full GitHub PR URL. " +
      "Agent turn diffs are not implemented yet."
    );
  }
}

async function runDxInTerminal(
  ctx: ExtensionCommandContext,
  dx: string,
  argv: string[],
): Promise<DxRunResult | undefined> {
  return ctx.ui.custom<DxRunResult>((tui, _theme, _keybindings, done) => {
    let result: DxRunResult;

    try {
      tui.stop();
      process.stdout.write("\x1b[2J\x1b[H");

      const child = spawnSync(dx, argv, {
        cwd: ctx.cwd,
        env: process.env,
        stdio: "inherit",
      });

      result = {
        status: child.status,
        signal: child.signal,
        error: child.error ? errorMessage(child.error) : undefined,
      };
    } catch (error) {
      result = {
        status: null,
        signal: null,
        error: errorMessage(error),
      };
    } finally {
      tui.start();
      tui.requestRender(true);
    }

    done(result);
    return { render: () => [], invalidate: () => {} };
  });
}

export function parseCommandLine(input: string): string[] {
  const args: string[] = [];
  let current = "";
  let quote: "'" | '"' | undefined;
  let escaped = false;
  let tokenStarted = false;

  for (const character of input) {
    if (escaped) {
      current += character;
      escaped = false;
      tokenStarted = true;
      continue;
    }

    if (quote === "'") {
      if (character === "'") {
        quote = undefined;
      } else {
        current += character;
      }
      tokenStarted = true;
      continue;
    }

    if (quote === '"') {
      if (character === '"') {
        quote = undefined;
      } else if (character === "\\") {
        escaped = true;
      } else {
        current += character;
      }
      tokenStarted = true;
      continue;
    }

    if (character === "\\") {
      escaped = true;
      tokenStarted = true;
      continue;
    }

    if (character === "'" || character === '"') {
      quote = character;
      tokenStarted = true;
      continue;
    }

    if (/\s/.test(character)) {
      if (tokenStarted) {
        args.push(current);
        current = "";
        tokenStarted = false;
      }
      continue;
    }

    current += character;
    tokenStarted = true;
  }

  if (escaped) {
    current += "\\";
  }

  if (quote) {
    throw new Error(
      `Unterminated ${quote === "'" ? "single" : "double"} quote in /diff arguments.`,
    );
  }

  if (tokenStarted) {
    args.push(current);
  }

  return args;
}

export function dxInvocationNeedsGit(argv: string[]): boolean {
  if (argv.some((arg) => ["--help", "-h", "--version", "-V"].includes(arg))) {
    return false;
  }

  for (let index = 0; index < argv.length; index++) {
    const arg = argv[index];
    if (arg === "--patch" || arg?.startsWith("--patch=")) {
      return false;
    }

    if (arg === "--pr") {
      const target = argv[index + 1];
      return target ? !isGitHubPullRequestUrl(target) : false;
    }

    if (arg?.startsWith("--pr=")) {
      return !isGitHubPullRequestUrl(arg.slice("--pr=".length));
    }
  }

  return true;
}

function repoPathFromArgs(argv: string[]): string | null | undefined {
  for (let index = 0; index < argv.length; index++) {
    const arg = argv[index];
    if (arg === "--repo" || arg === "-r") {
      return repoPathValue(argv[index + 1]);
    }
    if (arg?.startsWith("--repo=")) {
      return repoPathValue(arg.slice("--repo=".length));
    }
    if (arg?.startsWith("-r")) {
      const value = arg.slice("-r".length);
      return repoPathValue(value.startsWith("=") ? value.slice("=".length) : value);
    }
  }
  return undefined;
}

function repoPathValue(value: string | undefined): string | null {
  return value ? value : null;
}

function stdinPatchRequested(argv: string[]): boolean {
  for (let index = 0; index < argv.length; index++) {
    const arg = argv[index];
    if (arg === "--patch" && argv[index + 1] === "-") {
      return true;
    }
    if (arg === "--patch=-") {
      return true;
    }
  }
  return false;
}

function isGitHubPullRequestUrl(target: string): boolean {
  const value = target.trim();
  const withoutScheme = value.startsWith("https://")
    ? value.slice("https://".length)
    : value.startsWith("http://")
      ? value.slice("http://".length)
      : value;

  const path = withoutScheme.startsWith("github.com/")
    ? withoutScheme.slice("github.com/".length).split(/[?#]/, 1)[0]
    : undefined;
  if (!path) {
    return false;
  }

  const [owner, repo, marker, number] = path.split("/");
  return (
    validGitHubPathSegment(owner) &&
    validGitHubPathSegment(repo) &&
    marker === "pull" &&
    typeof number === "string" &&
    /^[0-9]+$/.test(number) &&
    !/^0+$/.test(number)
  );
}

function validGitHubPathSegment(segment: string | undefined): boolean {
  return typeof segment === "string" && /^[A-Za-z0-9._-]+$/.test(segment);
}

function report(ctx: ExtensionCommandContext, message: string, level: NotifyLevel): void {
  if (ctx.hasUI) {
    ctx.ui.notify(message, level);
    return;
  }

  const prefix = level === "error" ? "error" : level === "warning" ? "warning" : "info";
  console.error(`pi-dx ${prefix}: ${message}`);
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
