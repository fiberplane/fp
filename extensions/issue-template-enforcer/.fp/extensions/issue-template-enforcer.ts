/**
 * Issue Template Enforcer Extension
 *
 * Validates that new issues contain required sections in their description.
 * When an issue is created, this pre-hook checks:
 * 1. Description meets a minimum length
 * 2. Required markdown sections (## headings) are present
 *
 * Issues created with a "skip" status (e.g. backlog) bypass validation,
 * allowing quick issue dumps without friction.
 */

import type {
  FpExtensionContext,
  HookIssueContext,
} from "@fiberplane/extensions";

function parseCsv(raw: string): string[] {
  return raw
    .split(",")
    .map((value) => value.trim())
    .filter((value) => value.length > 0);
}

/**
 * Find which required sections are missing from a description.
 * Matches markdown headings at any level (##, ###, etc.) case-insensitively.
 */
function findMissingSections(description: string, required: string[]): string[] {
  const normalizedDescription = description.toLowerCase();
  const missing: string[] = [];

  for (const section of required) {
    // Match "## Section Name" at any heading level
    const pattern = new RegExp(`^#{1,6}\\s+${escapeRegex(section.toLowerCase())}`, "m");

    if (!pattern.test(normalizedDescription)) {
      missing.push(section);
    }
  }

  return missing;
}

function escapeRegex(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Build a helpful template skeleton showing the required sections.
 */
function buildTemplateSkeleton(sections: string[]): string {
  const parts: string[] = [];

  for (const section of sections) {
    parts.push(`## ${section}\n\n<!-- Describe ${section.toLowerCase()} here -->\n`);
  }

  return parts.join("\n");
}

export default function issueTemplateEnforcer(fp: FpExtensionContext) {
  const requiredSections = parseCsv(fp.config.get("required_sections", "Context,Acceptance Criteria"));
  const minDescriptionLength = Number.parseInt(
    fp.config.get("min_description_length", "30"),
    10,
  );
  const skipStatuses = new Set(parseCsv(fp.config.get("skip_statuses", "")));

  fp.log.info(
    `[issue-template-enforcer] loaded — required sections: ${requiredSections.join(", ")}`,
  );

  fp.on("issue:creating", ({ issue }: HookIssueContext) => {
    // Allow quick issue dumps for certain statuses
    if (skipStatuses.has(issue.status)) {
      fp.log.debug(
        `Skipping validation for issue "${issue.title}" — status "${issue.status}" is in skip list`,
      );
      return;
    }

    const description = issue.description ?? "";

    // Check minimum description length
    if (description.length < minDescriptionLength) {
      const skeleton = buildTemplateSkeleton(requiredSections);

      return {
        code: "DESCRIPTION_TOO_SHORT",
        message: [
          `Issue description is too short (${description.length}/${minDescriptionLength} characters).`,
          "",
          "Please include the following sections:",
          "",
          skeleton,
        ].join("\n"),
      };
    }

    // Check for required sections
    const missing = findMissingSections(description, requiredSections);

    if (missing.length > 0) {
      const skeleton = buildTemplateSkeleton(missing);

      return {
        code: "MISSING_SECTIONS",
        message: [
          `Issue is missing ${missing.length} required section(s): ${missing.join(", ")}.`,
          "",
          "Please add the following to your description:",
          "",
          skeleton,
        ].join("\n"),
      };
    }

    fp.log.debug(`Issue "${issue.title}" passed template validation`);
  });
}
