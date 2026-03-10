import type { FpExtensionContext } from "@fiberplane/extensions";

function parseCsv(raw: string): Set<string> {
  const values = raw
    .split(",")
    .map((value) => value.trim())
    .filter((value) => value.length > 0);

  return new Set(values);
}

function toBoolean(value: string): boolean {
  const normalized = value.trim().toLowerCase();
  return normalized === "true" || normalized === "1" || normalized === "yes";
}

export default function postCreateAutomation(fp: FpExtensionContext) {
  const triggerStatuses = parseCsv(fp.config.get("trigger_statuses", "backlog,todo"));
  const welcomeComment = fp.config.get(
    "welcome_comment",
    "Thanks for opening this issue. Add context, acceptance criteria, and links.",
  );
  const createFollowup = toBoolean(fp.config.get("create_followup", "false"));
  const followupPrefix = fp.config.get("followup_title_prefix", "Research: ");

  fp.on("issue:created", async ({ issue }) => {
    if (!triggerStatuses.has(issue.status)) {
      return;
    }

    try {
      await fp.comments.create(issue.id, welcomeComment);
      fp.log.info(`[post-create-automation] posted welcome comment for ${issue.id}`);
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      fp.log.warn(`[post-create-automation] failed to post welcome comment: ${message}`);
    }

    if (!createFollowup || issue.parent !== null) {
      return;
    }

    try {
      const followup = await fp.issues.create({
        title: `${followupPrefix}${issue.title}`,
        parent: issue.id,
        status: "todo",
      });
      fp.log.info(`[post-create-automation] created follow-up issue ${followup.id}`);
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      fp.log.warn(`[post-create-automation] failed to create follow-up issue: ${message}`);
    }
  });
}
