import { spawn, spawnSync } from "node:child_process";
import { accessSync, constants, statSync } from "node:fs";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { delimiter, dirname, join, parse, resolve as resolvePath } from "node:path";
import type {
  ExtensionAPI,
  ExtensionCommandContext,
  ExtensionContext,
} from "@earendil-works/pi-coding-agent";

const INSTALL_HINT =
  "Install dx with:\n" +
  "curl -fsSL https://raw.githubusercontent.com/phongndo/dx/main/scripts/install.sh | sh";

type NotifyLevel = "info" | "warning" | "error";

type DxRunResult = {
  status: number | null;
  signal: string | null;
  error?: string;
};

type GitSnapshot = {
  repo: string;
  tree: string;
};

type ActivePrompt = {
  id: string;
  index: number;
  text: string;
  before?: GitSnapshot;
};

type ActiveTurn = {
  prompt: ActivePrompt;
  turnIndex: number;
  before?: GitSnapshot;
};

type StoredTimelineDiff = {
  version: 1;
  kind: "prompt" | "turn";
  id: string;
  label: string;
  patchPath: string;
  repo: string;
  promptId: string;
  promptIndex: number;
  promptText: string;
  turnIndex?: number;
  files: number;
  additions: number;
  deletions: number;
  timestamp: number;
};

type DiffSetManifest = {
  version: 1;
  title: string;
  defaultItem?: string;
  items: DiffSetManifestItem[];
};

type DiffSetManifestItem =
  | {
      id: string;
      label: string;
      typeLabel?: string;
      detail?: string;
      kind: "worktree";
      repo: string;
    }
  | {
      id: string;
      label: string;
      typeLabel?: string;
      detail?: string;
      kind: "patch";
      repo?: string;
      patch: string;
    };

const TIMELINE_ENTRY_TYPE = "pi-dx.diff";

export default function piDx(pi: ExtensionAPI) {
  registerTimelineCapture(pi);

  pi.registerCommand("diff", {
    description: "Open the current or session timeline diff in dx",
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

  if (argv.length === 0) {
    await handleTimelineDiffCommand(ctx, dx);
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

  await runDxAndReportResult(ctx, dx, argv);
}

async function handleTimelineDiffCommand(ctx: ExtensionCommandContext, dx: string): Promise<void> {
  const timeline = timelineDiffs(ctx);
  const manifest = await buildDiffSetManifest(ctx, timeline);
  const hasCapturedTimelineDiffs = timeline.some((entry) => canAccess(entry.patchPath));
  if (manifest.items.length === 0) {
    report(ctx, "No current Git diff or captured Pi turn diffs found.", "warning");
    return;
  }

  if (!dxSupportsDiffSet(dx)) {
    if (!hasCapturedTimelineDiffs && manifest.items.some((item) => item.id === "current")) {
      report(
        ctx,
        "This dx binary does not support session timelines yet; opening the current diff instead.",
        "warning",
      );
      await runDxAndReportResult(ctx, dx, []);
      return;
    }

    report(
      ctx,
      `This dx binary does not support session timelines (--diffset). Update dx or set PI_DX_BIN to a dx binary built from this version.\n\n${INSTALL_HINT}`,
      "error",
    );
    return;
  }

  const manifestPath = await writeDiffSetManifest(ctx, manifest);
  await runDxAndReportResult(ctx, dx, ["--diffset", manifestPath]);
}

async function runDxAndReportResult(
  ctx: ExtensionCommandContext,
  dx: string,
  argv: string[],
): Promise<void> {
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

async function buildDiffSetManifest(
  ctx: ExtensionCommandContext,
  timeline: StoredTimelineDiff[],
): Promise<DiffSetManifest> {
  const items: DiffSetManifestItem[] = [];
  const capturedTimeline = timeline.filter((entry) => canAccess(entry.patchPath));
  items.push({
    id: "turns",
    label: "Turns",
    typeLabel: "Turns",
    detail: capturedTimeline.length > 0 ? "All" : undefined,
    kind: "patch",
    repo: ctx.cwd,
    patch: await writeTurnsPatch(ctx, capturedTimeline),
  });

  if (hasGitRepository(ctx.cwd, undefined)) {
    items.push({
      id: "current",
      label: "Current",
      typeLabel: "All changes",
      kind: "worktree",
      repo: ctx.cwd,
    });
  }

  for (const [index, entry] of capturedTimeline.entries()) {
    const label = String(index + 1);
    items.push({
      id: entry.id,
      label,
      typeLabel: "Turns",
      detail: label,
      kind: "patch",
      repo: entry.repo,
      patch: entry.patchPath,
    });
  }

  let latestTimelineItem: StoredTimelineDiff | undefined;
  for (let index = timeline.length - 1; index >= 0; index--) {
    const entry = timeline[index];
    if (entry && canAccess(entry.patchPath)) {
      latestTimelineItem = entry;
      break;
    }
  }
  return {
    version: 1,
    title: "Pi session diffs",
    defaultItem: latestTimelineItem?.id ?? "turns",
    items,
  };
}

async function writeTurnsPatch(
  ctx: ExtensionCommandContext,
  timeline: StoredTimelineDiff[],
): Promise<string> {
  const dir = await timelineStorageDir(ctx);
  const path = join(dir, `turns-${Date.now()}.diff`);
  const patches = await Promise.all(
    timeline.map(async (entry) => readFile(entry.patchPath, "utf8")),
  );
  await writeFile(path, patches.join("\n"), "utf8");
  return path;
}

async function writeDiffSetManifest(
  ctx: ExtensionCommandContext,
  manifest: DiffSetManifest,
): Promise<string> {
  const dir = await timelineStorageDir(ctx);
  const path = join(dir, `diffset-${Date.now()}.json`);
  await writeFile(path, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
  return path;
}

function timelineDiffs(ctx: ExtensionCommandContext): StoredTimelineDiff[] {
  return ctx.sessionManager.getBranch().flatMap((entry) => {
    if (entry.type !== "custom" || entry.customType !== TIMELINE_ENTRY_TYPE) {
      return [];
    }
    return isStoredTimelineDiff(entry.data) ? [entry.data] : [];
  });
}

function isStoredTimelineDiff(value: unknown): value is StoredTimelineDiff {
  if (!value || typeof value !== "object") {
    return false;
  }
  const entry = value as Partial<StoredTimelineDiff>;
  return (
    entry.version === 1 &&
    (entry.kind === "prompt" || entry.kind === "turn") &&
    typeof entry.id === "string" &&
    typeof entry.label === "string" &&
    typeof entry.patchPath === "string" &&
    typeof entry.repo === "string" &&
    typeof entry.promptId === "string" &&
    typeof entry.promptIndex === "number" &&
    typeof entry.promptText === "string" &&
    typeof entry.files === "number" &&
    typeof entry.additions === "number" &&
    typeof entry.deletions === "number" &&
    typeof entry.timestamp === "number"
  );
}

function dxBinary(): string {
  return process.env.PI_DX_BIN?.trim() || "dx";
}

function checkDxBinary(dx: string): string | undefined {
  if (!dx) {
    return `PI_DX_BIN is empty.\n\n${INSTALL_HINT}`;
  }

  if (!executableAvailable(dx)) {
    return `dx executable was not found (${dx}).\n\n${INSTALL_HINT}`;
  }
}

function dxSupportsDiffSet(dx: string): boolean {
  const result = spawnSync(dx, ["--help"], {
    encoding: "utf8",
    maxBuffer: 1024 * 1024,
  });
  return `${result.stdout ?? ""}\n${result.stderr ?? ""}`.includes("--diffset");
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

function checkGitRepository(cwd: string, repoPath: string | undefined): string | undefined {
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
    return `git is required for the default /diff mode but was not found: ${errorMessage(result.error)}`;
  }

  if (result.status !== 0 || result.stdout.trim() !== "true") {
    const target = repoPath ? `repository path ${repoPath}` : cwd;
    return (
      `No Git repository found at ${target}.\n\n` +
      "/diff opens Git-backed dx diffs unless you pass --patch <file>, --diffset <file>, " +
      "a full GitHub PR URL, or a captured Pi session turn diff is available."
    );
  }
}

function hasGitRepository(cwd: string, repoPath: string | undefined): boolean {
  return checkGitRepository(cwd, repoPath) === undefined;
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

function registerTimelineCapture(pi: ExtensionAPI): void {
  let promptCounter = 0;
  let activePrompt: ActivePrompt | undefined;
  let activeTurn: ActiveTurn | undefined;

  pi.on("session_start", (_event, ctx) => {
    promptCounter = maxStoredPromptIndex(ctx);
    activePrompt = undefined;
    activeTurn = undefined;
  });

  pi.on("before_agent_start", async (event, ctx) => {
    const index = promptCounter + 1;
    promptCounter = index;
    activePrompt = {
      id: `prompt-${Date.now()}-${index}`,
      index,
      text: event.prompt,
      before: await captureGitSnapshot(ctx),
    };
    activeTurn = undefined;
  });

  pi.on("turn_start", async (event, ctx) => {
    if (!activePrompt) {
      return;
    }
    activeTurn = {
      prompt: activePrompt,
      turnIndex: event.turnIndex,
      before: await captureGitSnapshot(ctx),
    };
  });

  pi.on("turn_end", async (event, ctx) => {
    const turn = activeTurn;
    activeTurn = undefined;
    if (!turn || !turn.before) {
      return;
    }

    const after = await captureGitSnapshot(ctx);
    if (!after) {
      return;
    }

    await persistSnapshotDiff(pi, ctx, {
      kind: "turn",
      prompt: turn.prompt,
      turnIndex: event.turnIndex,
      before: turn.before,
      after,
    });
  });

  pi.on("agent_end", async (_event, ctx) => {
    const prompt = activePrompt;
    activePrompt = undefined;
    activeTurn = undefined;
    if (!prompt?.before) {
      return;
    }

    const after = await captureGitSnapshot(ctx);
    if (!after) {
      return;
    }

    await persistSnapshotDiff(pi, ctx, {
      kind: "prompt",
      prompt,
      before: prompt.before,
      after,
    });
  });

  pi.on("session_shutdown", () => {
    activePrompt = undefined;
    activeTurn = undefined;
  });
}

function maxStoredPromptIndex(ctx: ExtensionContext): number {
  return ctx.sessionManager.getBranch().reduce((max, entry) => {
    if (entry.type !== "custom" || entry.customType !== TIMELINE_ENTRY_TYPE) {
      return max;
    }
    if (!isStoredTimelineDiff(entry.data)) {
      return max;
    }
    return Math.max(max, entry.data.promptIndex);
  }, 0);
}

async function captureGitSnapshot(ctx: ExtensionContext): Promise<GitSnapshot | undefined> {
  const repo = await gitRepositoryRoot(ctx.cwd);
  if (!repo) {
    return undefined;
  }

  try {
    const tree = await writeWorktreeTree(repo);
    return { repo, tree };
  } catch {
    return undefined;
  }
}

async function gitRepositoryRoot(cwd: string): Promise<string | undefined> {
  const result = await runCommand("git", ["rev-parse", "--show-toplevel"], { cwd });
  if (result.status !== 0) {
    return undefined;
  }
  const root = result.stdout.trim();
  return root || undefined;
}

async function writeWorktreeTree(repo: string): Promise<string> {
  const temp = await mkdtemp(join(tmpdir(), "pi-dx-index-"));
  const indexPath = join(temp, "index");
  const env = { GIT_INDEX_FILE: indexPath };

  try {
    const hasHead = await runCommand("git", ["rev-parse", "--verify", "--quiet", "HEAD"], {
      cwd: repo,
    });
    if (hasHead.status === 0) {
      await runCommandOk("git", ["read-tree", "HEAD"], { cwd: repo, env });
    } else {
      await runCommandOk("git", ["read-tree", "--empty"], { cwd: repo, env });
    }

    await runCommandOk("git", ["add", "-A", "--", "."], { cwd: repo, env });
    const tree = await runCommandOk("git", ["write-tree"], { cwd: repo, env });
    return tree.stdout.trim();
  } finally {
    await rm(temp, { recursive: true, force: true });
  }
}

async function persistSnapshotDiff(
  pi: ExtensionAPI,
  ctx: ExtensionContext,
  input:
    | {
        kind: "prompt";
        prompt: ActivePrompt;
        before: GitSnapshot;
        after: GitSnapshot;
      }
    | {
        kind: "turn";
        prompt: ActivePrompt;
        turnIndex: number;
        before: GitSnapshot;
        after: GitSnapshot;
      },
): Promise<void> {
  try {
    if (input.before.repo !== input.after.repo) {
      return;
    }
    if (input.before.tree === input.after.tree) {
      return;
    }

    const diff = await diffTrees(input.before.repo, input.before.tree, input.after.tree);
    if (!diff.trim()) {
      return;
    }

    const stats = patchStats(diff);
    const label =
      input.kind === "turn"
        ? `T${input.prompt.index}.${input.turnIndex + 1}`
        : `Prompt ${input.prompt.index}`;
    const id =
      input.kind === "turn"
        ? `${input.prompt.id}-turn-${input.turnIndex + 1}`
        : `${input.prompt.id}-prompt`;
    const dir = await timelineStorageDir(ctx);
    const patchPath = join(dir, `${safeFileName(id)}.diff`);
    await writeFile(patchPath, diff, "utf8");

    const entry: StoredTimelineDiff = {
      version: 1,
      kind: input.kind,
      id,
      label,
      patchPath,
      repo: input.before.repo,
      promptId: input.prompt.id,
      promptIndex: input.prompt.index,
      promptText: input.prompt.text,
      turnIndex: input.kind === "turn" ? input.turnIndex : undefined,
      files: stats.files,
      additions: stats.additions,
      deletions: stats.deletions,
      timestamp: Date.now(),
    };

    pi.appendEntry(TIMELINE_ENTRY_TYPE, entry);
  } catch {
    // Diff timeline capture must never fail the agent turn.
  }
}

async function diffTrees(repo: string, before: string, after: string): Promise<string> {
  const result = await runCommandOk("git", ["diff", "--binary", "--find-renames", before, after], {
    cwd: repo,
  });
  return result.stdout;
}

async function timelineStorageDir(ctx: ExtensionContext): Promise<string> {
  const sessionFile = ctx.sessionManager.getSessionFile();
  const dir = sessionFile
    ? `${sessionFile}.dx`
    : join(tmpdir(), `pi-dx-${ctx.sessionManager.getSessionId()}`);
  await mkdir(dir, { recursive: true });
  return dir;
}

function safeFileName(value: string): string {
  return value.replace(/[^A-Za-z0-9._-]+/g, "-");
}

function patchStats(patch: string): { files: number; additions: number; deletions: number } {
  const files = new Set<string>();
  let additions = 0;
  let deletions = 0;

  for (const line of patch.split("\n")) {
    if (line.startsWith("diff --git ")) {
      files.add(line);
      continue;
    }
    if (line.startsWith("+++") || line.startsWith("---")) {
      continue;
    }
    if (line.startsWith("+")) {
      additions++;
    } else if (line.startsWith("-")) {
      deletions++;
    }
  }

  return { files: files.size, additions, deletions };
}

type CommandResult = {
  status: number | null;
  signal: string | null;
  stdout: string;
  stderr: string;
  error?: string;
};

async function runCommandOk(
  command: string,
  args: string[],
  options: { cwd: string; env?: NodeJS.ProcessEnv },
): Promise<CommandResult> {
  const result = await runCommand(command, args, options);
  if (result.error) {
    throw new Error(result.error);
  }
  if (result.status !== 0) {
    throw new Error(result.stderr.trim() || `${command} exited with status ${result.status}`);
  }
  return result;
}

function runCommand(
  command: string,
  args: string[],
  options: { cwd: string; env?: NodeJS.ProcessEnv },
): Promise<CommandResult> {
  return new Promise((resolve) => {
    const child = spawn(command, args, {
      cwd: options.cwd,
      env: { ...process.env, ...options.env },
      stdio: ["ignore", "pipe", "pipe"],
    });
    const stdout: Buffer[] = [];
    const stderr: Buffer[] = [];
    let settled = false;
    const finish = (result: Omit<CommandResult, "stdout" | "stderr">) => {
      if (settled) {
        return;
      }
      settled = true;
      resolve({
        ...result,
        stdout: Buffer.concat(stdout).toString("utf8"),
        stderr: Buffer.concat(stderr).toString("utf8"),
      });
    };

    child.stdout?.on("data", (chunk: Buffer) => stdout.push(chunk));
    child.stderr?.on("data", (chunk: Buffer) => stderr.push(chunk));
    child.once("error", (error) => {
      finish({ status: null, signal: null, error: errorMessage(error) });
    });
    child.once("exit", (status, signal) => {
      finish({ status, signal });
    });
  });
}

async function runDxInTerminal(
  ctx: ExtensionCommandContext,
  dx: string,
  argv: string[],
): Promise<DxRunResult | undefined> {
  return ctx.ui.custom<DxRunResult>(async (tui, _theme, _keybindings, done) => {
    let result: DxRunResult;

    try {
      tui.stop();
      process.stdout.write("\x1b[2J\x1b[H");

      const child = spawn(dx, argv, {
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

function waitForChild(child: ReturnType<typeof spawn>): Promise<DxRunResult> {
  return new Promise((resolve) => {
    let settled = false;
    const finish = (result: DxRunResult) => {
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
    if (arg === "--diffset" || arg?.startsWith("--diffset=")) {
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
