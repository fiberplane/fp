import { spawn } from "node:child_process";
import { rm } from "node:fs/promises";
import { resolve } from "node:path";
import type { FpExtensionContext } from "@fiberplane/extensions";

interface CommandResult {
  ok: boolean;
  output: string;
}

async function runCmd(command: string, args: string[], cwd: string): Promise<CommandResult> {
  return await new Promise((complete) => {
    const proc = spawn(command, args, {
      cwd,
      stdio: ["ignore", "pipe", "pipe"],
      env: { ...process.env },
    });

    let stdout = "";
    let stderr = "";

    if (proc.stdout) {
      proc.stdout.on("data", (chunk: Buffer | string) => {
        stdout += chunk.toString();
      });
    }

    if (proc.stderr) {
      proc.stderr.on("data", (chunk: Buffer | string) => {
        stderr += chunk.toString();
      });
    }

    proc.once("error", (error) => {
      complete({
        ok: false,
        output: error.message,
      });
    });

    proc.once("close", (code) => {
      const output = [stdout, stderr]
        .filter((value) => value.length > 0)
        .join("\n")
        .trim();
      complete({
        ok: code === 0,
        output,
      });
    });
  });
}

function parseCommand(raw: string): { command: string; args: string[] } | null {
  const parts = raw.match(/"(?:\\.|[^"])*"|'(?:\\.|[^'])*'|\S+/g) ?? [];

  if (parts.length === 0) {
    return null;
  }

  const normalized = parts.map((part) => {
    if (
      (part.startsWith('"') && part.endsWith('"')) ||
      (part.startsWith("'") && part.endsWith("'"))
    ) {
      return part.slice(1, -1);
    }

    return part;
  });

  return {
    command: normalized[0],
    args: normalized.slice(1),
  };
}

/**
 * Sanitize a title into a valid branch/bookmark name.
 */
function sanitizeBranchName(title: string): string {
  return title
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/-{2,}/g, "-")
    .replace(/^-|-$/g, "");
}

/**
 * Derive a bookmark name for an epic from its title.
 */
function bookmarkName(title: string): string {
  return `epic/${sanitizeBranchName(title)}`;
}

/**
 * Derive a workspace directory name from an issue ID.
 */
function workspaceName(issueId: string): string {
  const shortId = issueId.slice(-8);
  return `ws-${shortId}`;
}

/**
 * jj-lifecycle extension
 *
 * Automates Jujutsu (jj) VCS operations based on issue lifecycle events.
 */
export default function jjLifecycle(fp: FpExtensionContext) {
  const projectDir = fp.projectDir;
  const installCmd = fp.config.get("install_cmd", "bun install");

  fp.on("issue:status:changing", ({ issue, to }) => {
    const isEpic = !issue.parent;

    if (to !== "in-progress") {
      return;
    }

    if (isEpic) {
      const name = bookmarkName(issue.title);
      fp.log.info(`[jj-lifecycle] Will create bookmark "${name}" for epic "${issue.title}"`);
      return;
    }

    const wsName = workspaceName(issue.id);
    fp.log.info(`[jj-lifecycle] Will create workspace "${wsName}" for task "${issue.title}"`);
  });

  fp.on("issue:status:changed", async ({ issue, to }) => {
    const isEpic = !issue.parent;

    if (isEpic && to === "in-progress") {
      const name = bookmarkName(issue.title);
      const result = await runCmd("jj", ["bookmark", "create", name], projectDir);

      if (result.ok) {
        fp.log.info(`[jj-lifecycle] Created bookmark "${name}"`);
        return;
      }

      fp.log.warn(`[jj-lifecycle] Failed to create bookmark "${name}": ${result.output}`);
      return;
    }

    if (!isEpic && to === "in-progress") {
      const wsName = workspaceName(issue.id);
      const wsPath = `../${wsName}`;
      const wsAbsPath = resolve(projectDir, wsPath);

      const addResult = await runCmd("jj", ["workspace", "add", wsPath], projectDir);

      if (!addResult.ok) {
        fp.log.warn(`[jj-lifecycle] Failed to create workspace "${wsName}": ${addResult.output}`);
        return;
      }

      fp.log.info(`[jj-lifecycle] Created workspace "${wsName}" at ${wsPath}`);

      const parsedInstall = parseCommand(installCmd);

      if (!parsedInstall) {
        fp.log.warn(`[jj-lifecycle] install_cmd is empty, skipping install in "${wsName}"`);
        return;
      }

      const installResult = await runCmd(parsedInstall.command, parsedInstall.args, wsAbsPath);

      if (installResult.ok) {
        fp.log.info(`[jj-lifecycle] Ran "${installCmd}" in workspace "${wsName}"`);
        return;
      }

      fp.log.warn(`[jj-lifecycle] Install command failed in "${wsName}": ${installResult.output}`);
      return;
    }

    if (!isEpic && to === "done") {
      const wsName = workspaceName(issue.id);
      const wsPath = `../${wsName}`;
      const wsAbsPath = resolve(projectDir, wsPath);

      const forgetResult = await runCmd("jj", ["workspace", "forget", wsName], projectDir);

      if (forgetResult.ok) {
        fp.log.info(`[jj-lifecycle] Forgot workspace "${wsName}"`);
      } else {
        fp.log.warn(
          `[jj-lifecycle] Failed to forget workspace "${wsName}": ${forgetResult.output}`,
        );
      }

      try {
        await rm(wsAbsPath, {
          recursive: true,
          force: true,
        });
        fp.log.info(`[jj-lifecycle] Removed workspace directory "${wsPath}"`);
      } catch (error: unknown) {
        const message = error instanceof Error ? error.message : String(error);
        fp.log.warn(`[jj-lifecycle] Failed to remove directory "${wsPath}": ${message}`);
      }

      return;
    }

    if (isEpic && to === "done") {
      const name = bookmarkName(issue.title);
      const result = await runCmd("jj", ["bookmark", "delete", name], projectDir);

      if (result.ok) {
        fp.log.info(`[jj-lifecycle] Deleted bookmark "${name}"`);
        return;
      }

      fp.log.warn(`[jj-lifecycle] Failed to delete bookmark "${name}": ${result.output}`);
    }
  });
}
