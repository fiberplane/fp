/**
 * Custom Properties Example
 *
 * Showcases every property display type and visual treatment:
 * - select (single pick dropdown)
 * - multiselect (multi pick with chips)
 * - text (inline editable string)
 *
 * Option visual treatments are inferred from shape:
 * - icon + color → colored chip
 * - icon only   → icon + plain label
 * - color only  → colored dot + label
 * - neither     → plain text
 */

import type { FpExtensionContext } from "@fiberplane/extensions";

export default async function customProperties(fp: FpExtensionContext) {
  // Select with icon + color → renders as colored chips
  await fp.issues.registerProperty("environment", {
    label: "Environment",
    icon: "server",
    display: fp.ui.properties.select(
      fp.ui.properties.option("production", {
        label: "Production",
        icon: "shield",
        color: "destructive",
      }),
      fp.ui.properties.option("staging", {
        label: "Staging",
        icon: "flask-conical",
        color: "warning",
      }),
      fp.ui.properties.option("development", { label: "Development", icon: "code", color: "blue" }),
    ),
  });

  // Select with color only → renders as colored dots
  await fp.issues.registerProperty("severity", {
    label: "Severity",
    icon: "alert-triangle",
    display: fp.ui.properties.select(
      fp.ui.properties.option("critical", { label: "Critical", color: "destructive" }),
      fp.ui.properties.option("high", { label: "High", color: "red" }),
      fp.ui.properties.option("medium", { label: "Medium", color: "orange" }),
      fp.ui.properties.option("low", { label: "Low", color: "yellow" }),
      fp.ui.properties.option("info", { label: "Info", color: "blue" }),
    ),
  });

  // Select with icon only → renders as icon + plain label
  await fp.issues.registerProperty("type", {
    label: "Type",
    icon: "tag",
    display: fp.ui.properties.select(
      fp.ui.properties.option("bug", { label: "Bug", icon: "bug" }),
      fp.ui.properties.option("feature", { label: "Feature", icon: "sparkles" }),
      fp.ui.properties.option("chore", { label: "Chore", icon: "wrench" }),
      fp.ui.properties.option("docs", { label: "Docs", icon: "book-open" }),
    ),
  });

  // Select with plain options → renders as plain text
  await fp.issues.registerProperty("effort", {
    label: "Effort",
    icon: "gauge",
    display: fp.ui.properties.select(
      fp.ui.properties.option("xs", { label: "XS" }),
      fp.ui.properties.option("s", { label: "S" }),
      fp.ui.properties.option("m", { label: "M" }),
      fp.ui.properties.option("l", { label: "L" }),
      fp.ui.properties.option("xl", { label: "XL" }),
    ),
  });

  // Multiselect with icon + color → removable colored chips
  await fp.issues.registerProperty("labels", {
    label: "Labels",
    icon: "tags",
    display: fp.ui.properties.multiselect(
      fp.ui.properties.option("frontend", { label: "Frontend", icon: "layout", color: "purple" }),
      fp.ui.properties.option("backend", {
        label: "Backend",
        icon: "database",
        color: "turquoise",
      }),
      fp.ui.properties.option("infra", { label: "Infra", icon: "cloud", color: "mint" }),
      fp.ui.properties.option("design", { label: "Design", icon: "palette", color: "pink" }),
      fp.ui.properties.option("security", { label: "Security", icon: "lock", color: "red" }),
      fp.ui.properties.option("perf", { label: "Performance", icon: "zap", color: "lime" }),
    ),
  });

  // Multiselect with color only → colored dot chips
  await fp.issues.registerProperty("sprint-goals", {
    label: "Sprint Goals",
    icon: "target",
    display: fp.ui.properties.multiselect(
      fp.ui.properties.option("reliability", { label: "Reliability", color: "success" }),
      fp.ui.properties.option("velocity", { label: "Velocity", color: "orange" }),
      fp.ui.properties.option("ux", { label: "UX Polish", color: "blue" }),
    ),
  });

  // Text → inline editable string
  await fp.issues.registerProperty("estimate", {
    label: "Estimate",
    icon: "clock",
    display: fp.ui.properties.text(),
  });

  // Text → another example
  await fp.issues.registerProperty("external-url", {
    label: "External URL",
    icon: "external-link",
    display: fp.ui.properties.text(),
  });

  fp.log.info("[custom-properties] registered all property types");
}
