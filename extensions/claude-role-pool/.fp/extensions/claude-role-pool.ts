import { execSync, spawn } from "node:child_process";
import { basename } from "node:path";
import type { ExtensionIssue, FpExtensionContext } from "@fiberplane/extensions";

type Role = "planner" | "implementer" | "reviewer";
type AgentSelection = "none" | Role;

interface CommandResult {
  ok: boolean;
  code: number | null;
  stdout: string;
  stderr: string;
}

interface RolePoolController {
  refreshPool: () => Promise<void>;
  dispatchIssue: (role: Role, issue: ExtensionIssue) => Promise<void>;
}

interface RepositoryCatalogEntry {
  id: string;
  label: string;
  url: string;
  targetDir: string;
}

interface RepositorySelectionResolution {
  specs: RepositoryCatalogEntry[];
  unknownIds: string[];
}

const ROLES: readonly Role[] = ["planner", "implementer", "reviewer"];
const BASE_TEMPLATE = "docker/sandbox-templates:claude-code-docker";
const RECONCILE_INTERVAL_MS = 5 * 60 * 1000;
const SANDBOX_REPOSITORIES_ROOT = "/home/agent/.fp-sandbox/repositories";

const ROLE_INSTRUCTIONS: Record<Role, string> = {
  planner: "You are the planning sandbox. Focus on understanding the issue, decomposing work, identifying risks, and proposing the best execution plan. Prefer creating or refining child issues and comments over making broad code changes.",
  implementer: "You are the implementation sandbox. Focus on making the code changes required for the assigned issue. Keep work scoped to the assigned issue set, update fp status/comments as you progress, and run relevant checks.",
  reviewer: "You are the review sandbox. Focus on reading code, validating changes, identifying risks, and leaving review feedback. Prefer comments, summaries, and follow-up issues over direct code changes unless asked.",
};

function isSandboxRuntimeEnvironment(): boolean {
  return process.env.IS_SANDBOX === "1" || process.env.FP_SANDBOX_RUNTIME === "1";
}

export default async function claudeRolePool(fp: FpExtensionContext): Promise<void> {
  const repositoryCatalog = loadRepositoryCatalog(
    fp.config.get<unknown>("repository_catalog", fp.config.get<unknown>("repositories_catalog", [])),
  );

  await registerProperties(fp, repositoryCatalog);

  if (isSandboxRuntimeEnvironment()) {
    fp.log.info("[claude-role-pool] running inside sandbox; lifecycle controller disabled");
    return;
  }

  if (fp.runtime !== "desktop") {
    fp.log.info("[claude-role-pool] desktop runtime required for sandbox orchestration");
    return;
  }

  const controller = await makeController(fp, repositoryCatalog);

  const autoDispatchForSelection = async (
    issue: ExtensionIssue,
    selection: AgentSelection,
    trigger: "issue:created" | "issue:updated",
  ): Promise<void> => {
    if (!isRoleSelection(selection)) {
      fp.log.info(`[claude-role-pool] agent set to none for ${issue.id}; skipping auto-dispatch`);
      return;
    }

    fp.log.info(
      `[claude-role-pool] auto-dispatching ${issue.id} to ${selection} sandbox from ${trigger}`,
    );
    await controller.dispatchIssue(selection, issue);
  };

  fp.on("issue:created", async ({ issue }) => {
    const selection = parseAgentSelection(issue.properties?.agent);
    if (!selection) {
      return;
    }

    await autoDispatchForSelection(issue, selection, "issue:created");
  });

  fp.on("issue:updated", async ({ issue, updates }) => {
    if (!updates.properties) {
      return;
    }

    if ("agent" in updates.properties) {
      const selection = parseAgentSelection(updates.properties.agent);
      if (!selection) {
        fp.log.warn(
          `[claude-role-pool] ignoring unsupported agent value for ${issue.id}: ${String(updates.properties.agent)}`,
        );
        return;
      }

      await autoDispatchForSelection(issue, selection, "issue:updated");
      return;
    }

    if (!("repositories" in updates.properties)) {
      return;
    }

    const selection = parseAgentSelection(issue.properties?.agent);
    if (!selection || !isRoleSelection(selection)) {
      fp.log.info(
        `[claude-role-pool] repositories changed for ${issue.id} but no active agent role is assigned`,
      );
      return;
    }

    fp.log.info(
      `[claude-role-pool] repositories changed for ${issue.id}; reprovisioning ${selection} sandbox`,
    );
    await controller.dispatchIssue(selection, issue);
  });

  await fp.ui.registerAction({
    id: "claude-role-pool.refresh",
    label: "Claude Sandbox Pool: Refresh",
    icon: "container",
    keywords: ["claude", "sandbox", "refresh", "fp"],
    onExecute: async () => {
      await controller.refreshPool();
      await fp.ui.notify("Claude sandbox pool refreshed", {
        kind: "success",
        title: "Claude Sandbox Pool",
      });
    },
  });

  for (const role of ROLES) {
    await fp.ui.registerAction({
      id: `claude-role-pool.dispatch.${role}`,
      label: `Send to ${capitalize(role)} Sandbox`,
      icon: roleIcon(role),
      keywords: ["claude", "sandbox", role, "fp"],
      when: (ctx) => Boolean(ctx.issue),
      onExecute: async (ctx) => {
        const issue = ctx.issue as ExtensionIssue | undefined;
        if (!issue) {
          await fp.ui.notify("Open an issue first", {
            kind: "warning",
            title: "Claude Sandbox Pool",
          });
          return;
        }

        const currentSelection = parseAgentSelection(issue.properties?.agent);
        if (currentSelection === role) {
          await controller.dispatchIssue(role, issue);
          return;
        }

        await fp.issues.update(issue.id, {
          properties: {
            agent: role,
          },
        });
      },
    });
  }

  void controller.refreshPool();
  setInterval(() => {
    void controller.refreshPool();
  }, RECONCILE_INTERVAL_MS);
}

async function registerProperties(
  fp: FpExtensionContext,
  repositoryCatalog: readonly RepositoryCatalogEntry[],
): Promise<void> {
  await fp.issues.registerProperty("agent", {
    label: "Agent",
    icon: "bot",
    display: fp.ui.properties.select(...agentOptions(fp)),
  });

  await fp.issues.registerProperty("repositories", {
    label: "Repositories",
    icon: "folders",
    display: fp.ui.properties.multiselect(...repositoryOptions(fp, repositoryCatalog)),
  });
}

function ensureFullPath(): void {
  if (!process.env.PATH?.includes("/opt/homebrew/bin")) {
    try {
      const shellPath = execSync("zsh -lc 'echo $PATH'", { encoding: "utf8" }).trim();
      if (shellPath) {
        process.env.PATH = shellPath;
      }
    } catch {}
  }
}

async function makeController(
  fp: FpExtensionContext,
  repositoryCatalog: readonly RepositoryCatalogEntry[],
): Promise<RolePoolController> {
  ensureFullPath();
  const template = fp.config.get("template", process.env.FP_SANDBOX_TEMPLATE ?? BASE_TEMPLATE);
  const projectSlug = sanitizeName(basename(fp.projectDir));
  const repositoryCatalogById = new Map(repositoryCatalog.map((entry) => [entry.id, entry]));

  const roleSandboxName = (role: Role): string => `fp-${projectSlug}-${role}`;

  async function listSandboxes(): Promise<Set<string>> {
    const result = await runCommand("sbx", ["ls", "--quiet"], fp.projectDir);
    if (!result.ok) {
      throw new Error(`Could not list sandboxes. ${shortOutput(result)}`);
    }

    return new Set(
      result.stdout
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean),
    );
  }

  async function sbxExec(sandboxName: string, script: string, detached = false): Promise<CommandResult> {
    const args = ["exec"];
    if (detached) {
      args.push("-d");
    }
    args.push(sandboxName, "bash", "-lc", script);
    return runCommand("sbx", args, fp.projectDir);
  }

  async function ensureNetworkPolicy(): Promise<void> {
    const hosts = "setup.fp.dev,host.docker.internal:7878";
    const result = await runCommand("sbx", ["policy", "allow", "network", hosts], fp.projectDir);
    if (!result.ok) {
      fp.log.warn(`[claude-role-pool] failed to set network policy: ${shortOutput(result)}`);
    }
  }

  async function bootstrapSandbox(sandboxName: string): Promise<void> {
    const result = await sbxExec(sandboxName, [
      "set -euo pipefail",
      "curl -fsSL https://setup.fp.dev/install.sh | FP_INSTALL_DIR=/home/agent/.local/bin sh",
      `echo 'export FP_API_PORT=7878' >> ~/.bashrc`,
      `echo 'export FP_API_HOST=host.docker.internal' >> ~/.bashrc`,
      `echo 'cd ${sq(fp.projectDir)}' >> ~/.bashrc`,
    ].join("\n"));
    if (!result.ok) {
      throw new Error(`Failed to bootstrap sandbox ${sandboxName}. ${shortOutput(result)}`);
    }
  }

  async function provisionRepositories(
    sandboxName: string,
    specs: readonly RepositoryCatalogEntry[],
  ): Promise<void> {
    if (specs.length === 0) {
      return;
    }

    const lines = ["set -euo pipefail"];
    for (const spec of specs) {
      const targetDir = repositoryTargetPath(spec);
      const parentDir = targetDir.substring(0, targetDir.lastIndexOf("/"));
      lines.push(
        `mkdir -p ${sq(parentDir)}`,
        `if [ -d ${sq(targetDir + "/.git")} ]; then`,
        `  git -C ${sq(targetDir)} remote set-url origin ${sq(spec.url)}`,
        `  git -C ${sq(targetDir)} fetch --all --prune`,
        `else`,
        `  git clone ${sq(spec.url)} ${sq(targetDir)}`,
        `fi`,
      );
    }

    const result = await sbxExec(sandboxName, lines.join("\n"));
    if (!result.ok) {
      throw new Error(`Failed to provision repositories in ${sandboxName}. ${shortOutput(result)}`);
    }
  }

  async function ensureRoleSandbox(
    role: Role,
    repositorySpecs: readonly RepositoryCatalogEntry[] = [],
  ): Promise<string> {
    const sandboxName = roleSandboxName(role);
    const sandboxes = await listSandboxes();

    if (!sandboxes.has(sandboxName)) {
      await ensureNetworkPolicy();
      const createArgs = [
        "create",
        "--name",
        sandboxName,
        "--template",
        template,
        "claude",
        fp.projectDir,
      ];
      const created = await runCommand("sbx", createArgs, fp.projectDir);
      if (!created.ok) {
        throw new Error(`Failed to create sandbox ${sandboxName}. ${shortOutput(created)}`);
      }
      fp.log.info(`[claude-role-pool] created sandbox ${sandboxName}`);

      await bootstrapSandbox(sandboxName);
    }

    await provisionRepositories(sandboxName, repositorySpecs);

    return sandboxName;
  }

  async function refreshPool(): Promise<void> {
    try {
      for (const role of ROLES) {
        await ensureRoleSandbox(role);
      }
      fp.log.info("[claude-role-pool] sandbox pool refreshed");
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      fp.log.warn(`[claude-role-pool] refresh failed: ${message}`);
    }
  }

  async function dispatchIssue(role: Role, issue: ExtensionIssue): Promise<void> {
    try {
      const repositories = resolveIssueRepositories(issue, repositoryCatalogById);
      if (repositories.unknownIds.length > 0) {
        fp.log.warn(
          `[claude-role-pool] unknown repositories for ${issue.id}: ${repositories.unknownIds.join(", ")}`,
        );
      }

      const sandboxName = await ensureRoleSandbox(role, repositories.specs);
      const prompt = renderPrompt(role, issue.id);

      fp.log.info(`[claude-role-pool] dispatching ${issue.id} to ${sandboxName} (${role})`);

      const args = [
        "exec", "-d",
        "-e", "FP_API_PORT=7878",
        "-e", "FP_API_HOST=host.docker.internal",
        "-w", fp.projectDir,
        sandboxName,
        "claude", "-p", "--dangerously-skip-permissions", prompt,
      ];
      const result = await runCommand("sbx", args, fp.projectDir);
      if (!result.ok) {
        throw new Error(`Dispatch failed. ${shortOutput(result)}`);
      }

      await fp.comments.create(issue.id, `Dispatched to \`${sandboxName}\` (${role}).`);

      await fp.ui.notify(`Dispatched ${issue.id} to ${role} sandbox`, {
        kind: "success",
        title: "Claude Sandbox Pool",
      });
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      fp.log.error(`[claude-role-pool] dispatch failed: ${message}`);

      await fp.ui.notify(message, {
        kind: "error",
        title: "Claude Sandbox Pool",
      });
    }
  }

  return {
    refreshPool,
    dispatchIssue,
  };
}

function renderPrompt(role: Role, issueId: string): string {
  return `${ROLE_INSTRUCTIONS[role]} Start by running: fp context ${issueId} --include-children. Use fp skills (/fp-implement, /fp-plan, /fp-review) for the workflow. Record meaningful progress with fp comments.`;
}

function agentOptions(fp: FpExtensionContext) {
  return [
    fp.ui.properties.option("none", { label: "None", icon: "minus" }),
    fp.ui.properties.option("planner", { label: "Planner", icon: "map", color: "blue" }),
    fp.ui.properties.option("implementer", {
      label: "Implementer",
      icon: "hammer",
      color: "success",
    }),
    fp.ui.properties.option("reviewer", {
      label: "Reviewer",
      icon: "search-check",
      color: "purple",
    }),
  ] as const;
}

function repositoryOptions(
  fp: FpExtensionContext,
  repositoryCatalog: readonly RepositoryCatalogEntry[],
) {
  return repositoryCatalog.map((entry) =>
    fp.ui.properties.option(entry.id, {
      label: entry.label,
      icon: "folder",
    }),
  );
}

function loadRepositoryCatalog(raw: unknown): RepositoryCatalogEntry[] {
  if (Array.isArray(raw)) {
    return raw
      .map((entry) => parseRepositoryCatalogEntry(entry))
      .filter((entry): entry is RepositoryCatalogEntry => entry !== null);
  }

  if (isRecord(raw)) {
    return Object.entries(raw)
      .map(([id, value]) => parseRepositoryCatalogEntry(value, id))
      .filter((entry): entry is RepositoryCatalogEntry => entry !== null);
  }

  return [];
}

function parseRepositoryCatalogEntry(
  raw: unknown,
  fallbackId?: string,
): RepositoryCatalogEntry | null {
  if (!isRecord(raw)) {
    return null;
  }

  const id = readString(raw.id) ?? fallbackId;
  const url = readString(raw.url) ?? readString(raw.clone_url) ?? readString(raw.cloneUrl);
  if (!id || !url) {
    return null;
  }

  const label = readString(raw.label) ?? id;
  const targetDir = normalizeRepositoryTargetDir(
    readString(raw.target_dir) ?? readString(raw.targetDir) ?? id,
    id,
  );

  return {
    id,
    label,
    url,
    targetDir,
  };
}

function normalizeRepositoryTargetDir(value: string, fallbackId: string): string {
  const segments = value
    .replace(/\\/g, "/")
    .split("/")
    .map((segment) => segment.trim())
    .filter((segment) => segment.length > 0 && segment !== "." && segment !== "..");

  const normalized = segments
    .map((segment) => segment.replace(/[^a-zA-Z0-9._-]+/g, "-"))
    .filter((segment) => segment.length > 0)
    .join("/");

  return normalized || sanitizeName(fallbackId);
}

function resolveIssueRepositories(
  issue: ExtensionIssue,
  repositoryCatalogById: ReadonlyMap<string, RepositoryCatalogEntry>,
): RepositorySelectionResolution {
  const specs: RepositoryCatalogEntry[] = [];
  const unknownIds: string[] = [];

  for (const repositoryId of parseRepositorySelections(issue.properties?.repositories)) {
    const spec = repositoryCatalogById.get(repositoryId);
    if (!spec) {
      unknownIds.push(repositoryId);
      continue;
    }

    specs.push(spec);
  }

  return {
    specs,
    unknownIds,
  };
}

function parseRepositorySelections(value: unknown): string[] {
  const values = Array.isArray(value)
    ? value
    : typeof value === "string"
      ? [value]
      : [];

  return [...new Set(values.filter((entry): entry is string => typeof entry === "string").map((entry) => entry.trim()).filter(Boolean))];
}

function repositoryTargetPath(spec: RepositoryCatalogEntry): string {
  return `${SANDBOX_REPOSITORIES_ROOT}/${spec.targetDir}`;
}

function parseAgentSelection(value: unknown): AgentSelection | null {
  switch (value) {
    case "none":
    case "planner":
    case "implementer":
    case "reviewer":
      return value;
    default:
      return null;
  }
}

function isRoleSelection(value: AgentSelection): value is Role {
  return value !== "none";
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function readString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

function sanitizeName(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/-{2,}/g, "-")
    .replace(/^-|-$/g, "")
    .slice(0, 32);
}

function capitalize(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1);
}

function roleIcon(role: Role): string {
  switch (role) {
    case "planner":
      return "map";
    case "implementer":
      return "hammer";
    case "reviewer":
      return "search-check";
  }
}

/** Shell-quote a value for embedding in bash scripts. */
function sq(value: string): string {
  return `'${value.replace(/'/g, `'"'"'`)}'`;
}

function shortOutput(result: CommandResult): string {
  return [result.stdout, result.stderr]
    .map((value) => value.trim())
    .filter((value) => value.length > 0)
    .join(" ")
    .slice(0, 500);
}

async function runCommand(
  command: string,
  args: readonly string[],
  cwd: string,
  extraEnv?: Record<string, string>,
): Promise<CommandResult> {
  return await new Promise((resolve) => {
    const proc = spawn(command, [...args], {
      cwd,
      env: { ...process.env, ...extraEnv },
      stdio: ["ignore", "pipe", "pipe"],
    });

    let stdout = "";
    let stderr = "";

    proc.stdout?.on("data", (chunk: Buffer | string) => {
      stdout += chunk.toString();
    });

    proc.stderr?.on("data", (chunk: Buffer | string) => {
      stderr += chunk.toString();
    });

    proc.once("error", (error) => {
      resolve({
        ok: false,
        code: null,
        stdout,
        stderr: `${stderr}${stderr ? "\n" : ""}${error.message}`,
      });
    });

    proc.once("close", (code) => {
      resolve({
        ok: code === 0,
        code,
        stdout,
        stderr,
      });
    });
  });
}
