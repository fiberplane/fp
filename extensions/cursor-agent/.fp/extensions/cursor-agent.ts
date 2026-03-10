/**
 * Cursor Cloud Agent Extension
 *
 * Delegates issue work to Cursor's cloud coding agent.
 * Demonstrates the v2 extension APIs:
 * - fp.issues.registerProperty — typed custom properties with display hints
 * - fp.secrets — OS keychain credential storage
 * - fp.ui.registerAction — command palette and detail view actions
 * - fp.ui.notify — toast notifications
 */

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import type { ExtensionIssue, FpExtensionContext } from "@fiberplane/extensions";
import { z } from "zod";

// FIXME: replace with actual Cursor API base URL
const CURSOR_API_BASE = "https://api.cursor.com";
const POLL_INTERVAL_MS = 30_000;
const MAX_PROJECT_GUIDELINES_CHARS = 20_000;

const CursorAgentField = z.object({
  agentId: z.string(),
  status: z.enum(["running", "finished", "error"]),
  branch: z.string().optional(),
  prUrl: z.string().url().optional(),
  error: z.string().optional(),
});

type CursorAgent = z.infer<typeof CursorAgentField>;

export default async function cursorAgent(fp: FpExtensionContext) {
  // --- Data ---

  await fp.issues.registerProperty("cursor-agent", {
    label: "Cursor Status",
    icon: "box",
    display: fp.ui.properties.select(
      fp.ui.properties.option("CREATING", { label: "Creating", color: "blue" }),
      fp.ui.properties.option("RUNNING", { label: "Running", color: "yellow" }),
      fp.ui.properties.option("FINISHED", { label: "Finished", color: "success" }),
      fp.ui.properties.option("STOPPED", { label: "Stopped", color: "red" }),
    ),
    schema: CursorAgentField,
  });

  // --- Actions ---

  fp.ui.registerAction({
    id: "cursor:launch",
    label: "Send to Cursor Agent",
    icon: "sparkles",
    keywords: ["cursor", "agent", "ai", "code"],
    when: (ctx) => {
      const issue = ctx.issue as Record<string, unknown> | undefined;
      if (!issue) return false;
      const fields = issue.fields as Record<string, unknown> | undefined;
      return !fields?.["cursor-agent"];
    },
    onExecute: (ctx) => {
      const issue = ctx.issue as ExtensionIssue;
      return launchAgent(fp, issue);
    },
  });

  fp.ui.registerAction({
    id: "cursor:retry",
    label: "Retry Cursor Agent",
    icon: "refresh-cw",
    keywords: ["cursor", "retry"],
    when: (ctx) => {
      const issue = ctx.issue as Record<string, unknown> | undefined;
      if (!issue) return false;
      const fields = issue.fields as Record<string, unknown> | undefined;
      const agent = fields?.["cursor-agent"] as CursorAgent | undefined;
      return agent?.status === "error";
    },
    onExecute: async (ctx) => {
      const issue = ctx.issue as ExtensionIssue;
      await fp.issues.update(issue.id, { fields: { "cursor-agent": undefined } });
      await launchAgent(fp, issue);
    },
  });

  fp.ui.registerAction({
    id: "cursor:stop",
    label: "Stop Cursor Agent",
    icon: "square",
    keywords: ["cursor", "stop", "cancel"],
    when: (ctx) => {
      const issue = ctx.issue as Record<string, unknown> | undefined;
      if (!issue) return false;
      const fields = issue.fields as Record<string, unknown> | undefined;
      const agent = fields?.["cursor-agent"] as CursorAgent | undefined;
      return agent?.status === "running";
    },
    onExecute: (ctx) => {
      const issue = ctx.issue as ExtensionIssue;
      return stopAgent(fp, issue);
    },
  });

  fp.ui.registerAction({
    id: "cursor:open",
    label: "Open in Cursor",
    icon: "external-link",
    keywords: ["cursor", "open", "dashboard"],
    when: (ctx) => {
      const issue = ctx.issue as Record<string, unknown> | undefined;
      if (!issue) return false;
      const fields = issue.fields as Record<string, unknown> | undefined;
      return fields?.["cursor-agent"] != null;
    },
    onExecute: (ctx) => {
      const issue = ctx.issue as ExtensionIssue;
      const agent = issue.fields?.["cursor-agent"] as CursorAgent;
      // FIXME: replace with actual Cursor dashboard URL
      fp.log.info(`Open in browser: https://cursor.com/agents/${agent.agentId}`);
    },
  });
}

// --- Core logic ---

async function launchAgent(fp: FpExtensionContext, issue: ExtensionIssue) {
  const apiKey = await fp.secrets.get("api-key");
  if (!apiKey) {
    await fp.ui.notify("Cursor API key not configured. Set it in Extensions settings.", {
      kind: "error",
      title: "Cursor",
    });
    return;
  }

  const prompt = await buildPrompt(fp, issue);

  // FIXME: replace with actual Cursor agent launch endpoint
  const res = await cursorFetch(apiKey, "/v0/agents", {
    method: "POST",
    body: JSON.stringify({
      prompt,
      projectDir: fp.projectDir,
    }),
  });

  if (!res.ok) {
    await fp.ui.notify(`Failed to launch agent: ${res.statusText}`, {
      kind: "error",
      title: "Cursor",
    });
    return;
  }

  const data = (await res.json()) as { id: string; branch?: string };

  await fp.issues.update(issue.id, {
    status: "in-progress",
    fields: {
      "cursor-agent": {
        agentId: data.id,
        status: "running",
        branch: data.branch,
      },
    },
  });

  await fp.ui.notify(`Agent launched for ${issue.title}`, {
    kind: "success",
    title: "Cursor",
  });

  pollAgent(fp, issue.id, data.id, apiKey);
}

async function stopAgent(fp: FpExtensionContext, issue: ExtensionIssue) {
  const agent = issue.fields?.["cursor-agent"] as CursorAgent | undefined;
  if (!agent) return;

  const apiKey = await fp.secrets.get("api-key");
  if (!apiKey) return;

  // FIXME: replace with actual Cursor cancel endpoint
  await cursorFetch(apiKey, `/v0/agents/${agent.agentId}/cancel`, {
    method: "POST",
  });

  await fp.issues.update(issue.id, {
    fields: {
      "cursor-agent": { ...agent, status: "error" as const, error: "Cancelled by user" },
    },
  });

  await fp.ui.notify("Agent stopped", { kind: "warning", title: "Cursor" });
}

function pollAgent(fp: FpExtensionContext, issueId: string, agentId: string, apiKey: string) {
  const interval = setInterval(async () => {
    try {
      // FIXME: replace with actual Cursor agent status endpoint
      const res = await cursorFetch(apiKey, `/v0/agents/${agentId}`);
      if (!res.ok) {
        fp.log.warn(`Poll failed: ${res.statusText}`);
        return;
      }

      const data = (await res.json()) as {
        status: string;
        branch?: string;
        pullRequestUrl?: string;
        error?: string;
        summary?: string;
      };

      const status = data.status as "running" | "finished" | "error";

      await fp.issues.update(issueId, {
        fields: {
          "cursor-agent": {
            agentId,
            status,
            branch: data.branch,
            prUrl: data.pullRequestUrl,
            error: data.error,
          },
        },
      });

      if (status === "running") return;

      clearInterval(interval);

      if (status === "finished") {
        const autoComplete = fp.config.get("auto-complete", false);
        if (autoComplete) {
          await fp.issues.update(issueId, { status: "done" });
        }

        const summary = buildSummary(data);
        await fp.comments.create(issueId, summary);
        await fp.ui.notify("Agent finished", { kind: "success", title: "Cursor" });
      }

      if (status === "error") {
        await fp.ui.notify(`Agent failed: ${data.error}`, { kind: "error", title: "Cursor" });
      }
    } catch (err) {
      fp.log.error(`Poll error: ${err}`);
    }
  }, POLL_INTERVAL_MS);
}

// --- Helpers ---

async function buildPrompt(fp: FpExtensionContext, issue: ExtensionIssue): Promise<string> {
  const children = await fp.issues.list({ parent: issue.id });
  const comments = await fp.comments.list(issue.id);

  const sections: string[] = [];

  sections.push(`## Task: ${issue.title}`);

  sections.push(
    `### Issue Metadata\n- **ID:** ${issue.id}\n- **Status:** ${issue.status}\n- **Priority:** ${issue.priority ?? "none"}`,
  );

  if (issue.description.trim().length > 0) {
    sections.push(`### Description\n${issue.description}`);
  }

  if (children.length > 0) {
    const checklist = children.map(
      (child) => `- [${child.status === "done" ? "x" : " "}] ${child.title}`,
    );
    sections.push(`### Sub-tasks\n${checklist.join("\n")}`);
  }

  if (comments.length > 0) {
    const discussion = comments
      .map((comment) => `> **${comment.author}** (${comment.createdAt}): ${comment.content}`)
      .join("\n\n");
    sections.push(`### Discussion Context\n${discussion}`);
  }

  const guidelines = readProjectGuidelines(fp.projectDir);
  if (guidelines) {
    sections.push(`### Project Guidelines\n${guidelines}`);
  }

  sections.push(
    [
      "### Instructions",
      "- Keep changes focused on this issue.",
      "- Preserve existing architecture and coding conventions.",
      "- Run relevant checks/tests for touched code.",
      "- Provide a concise summary of what changed and why.",
    ].join("\n"),
  );

  return sections.join("\n\n");
}

function readProjectGuidelines(projectDir: string): string | null {
  const candidates = [
    path.join(projectDir, "AGENTS.md"),
    path.join(projectDir, ".cursor", "rules"),
  ];

  for (const candidate of candidates) {
    try {
      if (!existsSync(candidate)) {
        continue;
      }

      let content: string;

      if (statSync(candidate).isDirectory()) {
        const entries = readdirSync(candidate)
          .filter((entry) => !entry.startsWith("."))
          .sort();

        const parts: string[] = [];
        for (const entry of entries) {
          const entryPath = path.join(candidate, entry);
          if (!statSync(entryPath).isFile()) {
            continue;
          }

          const text = readFileSync(entryPath, "utf-8").trim();
          if (text.length > 0) {
            parts.push(`## ${entry}\n\n${text}`);
          }
        }

        content = parts.join("\n\n");
      } else {
        content = readFileSync(candidate, "utf-8").trim();
      }

      if (content.length === 0) {
        continue;
      }

      if (content.length > MAX_PROJECT_GUIDELINES_CHARS) {
        return `${content.slice(0, MAX_PROJECT_GUIDELINES_CHARS)}\n\n… (truncated)`;
      }

      return content;
    } catch {
      // Ignore unreadable guideline files.
    }
  }

  return null;
}

function buildSummary(data: Record<string, unknown>): string {
  const lines = ["## Cursor Agent Summary"];

  if (data.summary) {
    lines.push("", String(data.summary));
  }

  if (data.branch) {
    lines.push("", `**Branch:** \`${data.branch}\``);
  }

  if (data.pullRequestUrl) {
    lines.push(`**PR:** ${data.pullRequestUrl}`);
  }

  return lines.join("\n");
}

function cursorFetch(apiKey: string, path: string, init?: RequestInit): Promise<Response> {
  const encoded = btoa(`${apiKey}:`);
  return fetch(`${CURSOR_API_BASE}${path}`, {
    ...init,
    headers: {
      ...init?.headers,
      Authorization: `Basic ${encoded}`,
      "Content-Type": "application/json",
    },
  });
}
