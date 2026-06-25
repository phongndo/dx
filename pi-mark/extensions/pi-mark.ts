import { spawn, spawnSync } from "node:child_process";
import { accessSync, constants, statSync } from "node:fs";
import { delimiter, dirname, join, parse, resolve as resolvePath } from "node:path";
import type { ExtensionAPI, ExtensionCommandContext } from "@earendil-works/pi-coding-agent";

const INSTALL_HINT =
  "Install mark with:\n" +
  "curl -fsSL https://raw.githubusercontent.com/phongndo/mark/main/scripts/install.sh | sh";

type NotifyLevel = "info" | "warning" | "error";

type MarkRunResult = {
  status: number | null;
  signal: string | null;
  error?: string;
};

type MarkCommand = "diff" | "show" | "review" | "patch" | "help";

type MarkInvocation = {
  command: MarkCommand;
  argv: string[];
  cliArgs: string[];
  label: string;
};

export default function piMark(pi: ExtensionAPI) {
  pi.registerCommand("mark", {
    description: "Open mark diff reviewer",
    handler: async (args, ctx) => {
      await handleMarkCommand(args, ctx);
    },
  });
}

async function handleMarkCommand(args: string, ctx: ExtensionCommandContext): Promise<void> {
  if (ctx.mode !== "tui") {
    report(ctx, "/mark requires Pi interactive TUI mode.", "error");
    return;
  }

  let argv: string[];
  try {
    argv = parseCommandLine(args);
  } catch (error) {
    report(ctx, errorMessage(error), "error");
    return;
  }

  const invocation = markInvocation(argv);

  if (stdinPatchRequested(invocation.command, invocation.argv)) {
    report(
      ctx,
      `${stdinPatchSource()} cannot read a patch from stdin inside Pi. Write the patch to a file and run /mark patch <file>.`,
      "error",
    );
    return;
  }

  const mark = markBinary();
  const markError = checkMarkBinary(mark);
  if (markError) {
    report(ctx, markError, "error");
    return;
  }

  if (markInvocationNeedsGit(invocation.command, invocation.argv)) {
    const repoPath = repoPathFromArgs(invocation.argv);
    if (repoPath === null) {
      report(ctx, `${invocation.label} --repo requires a repository path.`, "error");
      return;
    }

    const gitError = checkGitRepository(invocation.label, ctx.cwd, repoPath);
    if (gitError) {
      report(ctx, gitError, "error");
      return;
    }
  }

  const result = await runMarkInTerminal(ctx, mark, markInvocationArgs(invocation));
  if (!result) {
    report(ctx, "mark did not return a result.", "error");
    return;
  }

  if (result.error) {
    report(ctx, `Failed to run mark: ${result.error}`, "error");
    return;
  }

  if (result.signal) {
    report(ctx, `mark terminated by signal ${result.signal}.`, "warning");
    return;
  }

  if (result.status !== 0) {
    report(ctx, `mark exited with status ${result.status}.`, "error");
  }
}

function markInvocation(argv: string[]): MarkInvocation {
  const first = argv[0];
  if (
    first === "diff" ||
    first === "show" ||
    first === "review" ||
    first === "patch" ||
    first === "help"
  ) {
    return {
      command: first,
      argv: argv.slice(1),
      cliArgs: argv,
      label: `/mark ${first}`,
    };
  }

  return {
    command: "diff",
    argv,
    cliArgs: argv,
    label: "/mark",
  };
}

function markBinary(): string {
  return process.env.PI_MARK_BIN?.trim() || "mark";
}

function checkMarkBinary(mark: string): string | undefined {
  if (!mark) {
    return `PI_MARK_BIN is empty.\n\n${INSTALL_HINT}`;
  }

  if (!executableAvailable(mark)) {
    return `mark executable was not found (${mark}).\n\n${INSTALL_HINT}`;
  }
}

function executableAvailable(command: string): boolean {
  if (looksLikePath(command)) {
    return executablePathAvailable(command);
  }

  for (const directory of (process.env.PATH ?? "").split(delimiter)) {
    if (executablePathAvailable(join(directory || ".", command))) {
      return true;
    }
  }

  return false;
}

function executablePathAvailable(path: string): boolean {
  return executablePathCandidates(path).some(canExecute);
}

function executablePathCandidates(path: string): string[] {
  if (process.platform !== "win32") {
    return [path];
  }

  const extensions = (process.env.PATHEXT || ".COM;.EXE;.BAT;.CMD").split(";").filter(Boolean);
  const lowerPath = path.toLowerCase();
  if (extensions.some((extension) => lowerPath.endsWith(extension.toLowerCase()))) {
    return [path];
  }

  return [path, ...extensions.map((extension) => `${path}${extension}`)];
}

function canExecute(path: string): boolean {
  try {
    accessSync(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function looksLikePath(command: string): boolean {
  return command.includes("/") || command.includes("\\");
}

function checkGitRepository(
  commandLabel: string,
  cwd: string,
  repoPath: string | undefined,
): string | undefined {
  if (hasGitMarker(cwd, repoPath)) {
    return undefined;
  }

  const gitArgs = repoPath
    ? ["-C", repoPath, "rev-parse", "--is-inside-work-tree"]
    : ["rev-parse", "--is-inside-work-tree"];
  const result = spawnSync("git", gitArgs, {
    cwd,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });

  if (result.error) {
    return `git is required for ${commandLabel} but was not found: ${errorMessage(result.error)}`;
  }

  if (result.status !== 0 || result.stdout.trim() !== "true") {
    const target = repoPath ? `repository path ${repoPath}` : cwd;
    return (
      `No Git repository found at ${target}.\n\n` +
      "/mark, /mark diff, /mark show, and numeric /mark review targets use Git-backed mark sources unless you run /mark patch <file> or /mark review <full GitHub PR URL>. " +
      "Agent turn diffs are not implemented yet."
    );
  }
}

function hasGitMarker(cwd: string, repoPath: string | undefined): boolean {
  let current = resolvePath(cwd, repoPath ?? ".");
  if (!isDirectory(current)) {
    return false;
  }

  const root = parse(current).root;

  while (true) {
    if (canAccess(join(current, ".git"))) {
      return true;
    }

    if (current === root) {
      break;
    }
    current = dirname(current);
  }

  return false;
}

function isDirectory(path: string): boolean {
  try {
    return statSync(path).isDirectory();
  } catch {
    return false;
  }
}

function canAccess(path: string): boolean {
  try {
    accessSync(path, constants.F_OK);
    return true;
  } catch {
    return false;
  }
}

async function runMarkInTerminal(
  ctx: ExtensionCommandContext,
  mark: string,
  argv: string[],
): Promise<MarkRunResult | undefined> {
  return ctx.ui.custom<MarkRunResult>(async (tui, _theme, _keybindings, done) => {
    let result: MarkRunResult;

    try {
      tui.stop();
      process.stdout.write("\x1b[2J\x1b[H");

      const child = spawn(mark, argv, {
        cwd: ctx.cwd,
        env: process.env,
        stdio: "inherit",
      });

      result = await waitForChild(child);
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

function waitForChild(child: ReturnType<typeof spawn>): Promise<MarkRunResult> {
  return new Promise((resolve) => {
    let settled = false;
    const finish = (result: MarkRunResult) => {
      if (settled) {
        return;
      }
      settled = true;
      resolve(result);
    };

    child.once("error", (error) => {
      finish({ status: null, signal: null, error: errorMessage(error) });
    });
    child.once("exit", (status, signal) => {
      finish({ status, signal });
    });
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
      `Unterminated ${quote === "'" ? "single" : "double"} quote in slash command arguments.`,
    );
  }

  if (tokenStarted) {
    args.push(current);
  }

  return args;
}

export function markInvocationNeedsGit(command: MarkCommand, argv: string[]): boolean {
  if (argv.some((arg) => ["--help", "-h"].includes(arg) || isVersionFlag(arg))) {
    return false;
  }

  if (command === "patch") {
    return false;
  }

  if (command === "help") {
    return false;
  }

  if (command === "review") {
    const target = targetFromArgs(argv, 0);
    return target ? !isGitHubPullRequestUrl(target) : false;
  }

  return true;
}

function patchTargetFromArgs(argv: string[]): string | undefined {
  return targetFromArgs(argv, 0);
}

function targetFromArgs(argv: string[], startIndex: number): string | undefined {
  for (let index = startIndex; index < argv.length; index++) {
    const arg = argv[index];
    if (arg === "--") {
      return argv[index + 1];
    }
    if (arg === "--stat" || arg === "-s" || arg === "--no-syntax") {
      continue;
    }
    if (arg === "--repo" || arg === "-r") {
      index++;
      continue;
    }
    if (arg.startsWith("--repo=") || (arg.startsWith("-r") && arg !== "-r")) {
      continue;
    }
    return arg;
  }
  return undefined;
}

function markInvocationArgs(invocation: MarkInvocation): string[] {
  const versionFlag = invocation.cliArgs.find(isVersionFlag);
  if (versionFlag) {
    return [versionFlag];
  }

  return invocation.cliArgs;
}

function stdinPatchSource(): string {
  return "/mark patch";
}

function isVersionFlag(arg: string): boolean {
  return arg === "--version" || arg === "-V";
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

function stdinPatchRequested(command: MarkCommand, argv: string[]): boolean {
  if (command === "patch") {
    return patchTargetFromArgs(argv) === "-";
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
  console.error(`pi-mark ${prefix}: ${message}`);
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
