import type { FpExtensionContext } from "@fiberplane/extensions";

export default function helloHooks(fp: FpExtensionContext) {
  const greetingPrefix = fp.config.get("greeting_prefix", "👋");

  fp.log.info(`[hello-hooks] loaded in ${fp.runtime}`);

  fp.on("issue:created", async ({ issue }) => {
    fp.log.info(`${greetingPrefix} issue created: ${issue.id} ${issue.title}`);

    const welcomeComment = fp.config.get(
      "welcome_comment",
      "Thanks for opening this issue. Add context, acceptance criteria, and links.",
    );
    await fp.comments.create(issue.id, welcomeComment);
  });

  fp.on("issue:status:changed", ({ issue, from, to }) => {
    fp.log.info(`${greetingPrefix} status changed: ${issue.id} ${from} → ${to}`);
  });

  fp.on("comment:created", ({ issueId }) => {
    fp.log.info(`${greetingPrefix} comment created for ${issueId}`);
  });
}
