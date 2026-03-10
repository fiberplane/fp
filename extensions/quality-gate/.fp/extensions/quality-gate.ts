import { spawn } from "node:child_process";
import type { FpExtensionContext } from "@fiberplane/extensions";

interface CheckCommand {
  command: string;
  args: string[];
}

/** Map of check names to commands. */
const CHECK_COMMANDS: Record<string, CheckCommand> = {
  test: {
    command: "bun",
    args: ["test"],
  },
  typecheck: {
    command: "bun",
    args: ["run", "typecheck"],
  },
  lint: {
    command: "bun",
    args: ["run", "lint"],
  },
};

interface CheckResult {
  name: string;
  ok: boolean;
  output: string;
}

async function runCheck(
  check: CheckCommand,
  cwd: string,
): Promise<{ ok: boolean; output: string }> {
  return await new Promise((complete) => {
    const proc = spawn(check.command, check.args, {
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
      const ok = code === 0;
      complete({
        ok,
        output: ok ? stdout : stderr || stdout,
      });
    });
  });
}

/**
 * Quality gate extension — blocks `→ done` transitions when checks fail.
 */
export default function qualityGate(fp: FpExtensionContext) {
  const projectDir = fp.projectDir;
  const rawChecks = fp.config.get("checks", "test,typecheck,lint");
  const checkNames = rawChecks
    .split(",")
    .map((check) => check.trim())
    .filter((check) => check.length > 0);

  fp.on("issue:status:changing", async ({ issue, from, to }) => {
    if (to !== "done") {
      return;
    }

    fp.log.info(
      `Quality gate: running ${checkNames.length} check(s) before ${issue.id} can move ${from} → ${to}`,
    );

    const results: CheckResult[] = [];

    for (const name of checkNames) {
      const check = CHECK_COMMANDS[name];

      if (!check) {
        fp.log.info(`Skipping unknown check "${name}" — no command mapped`);
        continue;
      }

      const printableCommand = [check.command, ...check.args].join(" ");
      fp.log.info(`Running check: ${name} (${printableCommand})`);

      const result = await runCheck(check, projectDir);
      results.push({ name, ...result });

      if (result.ok) {
        fp.log.info(`✓ ${name} passed`);
      } else {
        fp.log.info(`✗ ${name} failed`);
      }
    }

    const failures = results.filter((result) => !result.ok);

    if (failures.length === 0) {
      fp.log.info("All quality checks passed ✓");
      return;
    }

    const failedNames = failures.map((failure) => failure.name).join(", ");
    const details = failures
      .map((failure) => `── ${failure.name} ──\n${failure.output.trim()}`)
      .join("\n\n");

    return {
      code: "QUALITY_GATE_FAILED",
      message: `Cannot move to done — ${failures.length} check(s) failed: ${failedNames}\n\n${details}`,
    };
  });
}
