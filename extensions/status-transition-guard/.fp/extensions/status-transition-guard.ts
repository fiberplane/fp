import type { FpExtensionContext } from "@fiberplane/extensions";

function parseAllowedList(raw: string): string[] {
  return raw
    .split(",")
    .map((value) => value.trim())
    .filter((value) => value.length > 0);
}

export default function statusTransitionGuard(fp: FpExtensionContext) {
  const allowedTransitions: Record<string, string[]> = {
    backlog: parseAllowedList(fp.config.get("allow_backlog", "todo")),
    todo: parseAllowedList(fp.config.get("allow_todo", "in-progress,backlog")),
    "in-progress": parseAllowedList(fp.config.get("allow_in_progress", "done,todo")),
    done: parseAllowedList(fp.config.get("allow_done", "todo")),
  };

  fp.on("issue:status:changing", ({ from, to }) => {
    const allowed = allowedTransitions[from] ?? [];

    if (allowed.includes(to)) {
      return;
    }

    return {
      code: "TRANSITION_BLOCKED",
      message: `Transition blocked: ${from} → ${to}. Allowed from ${from}: ${allowed.join(", ") || "none"}.`,
    };
  });

  fp.log.info("[status-transition-guard] loaded");
}
