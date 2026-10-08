import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { isAbsolute, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";

const MNEMEX = process.env.MNEMEX ?? "mnemex";
const WRITE_TOOLS = new Set(["write", "edit"]);
const UNICODE_SPACES = new RegExp(`[${[0xa0, 0x2000].map((c) => String.fromCodePoint(c)).join("")}-${[0x200a, 0x202f, 0x205f, 0x3000].map((c) => String.fromCodePoint(c)).join("")}]`, "g");

type HookResponse = {
  decision?: string;
  reason?: string;
  hookSpecificOutput?: { additionalContext?: string };
};

// An empty MNEMEX_VAULT counts as unset, as mnemex reads it.
function vaultRoot(): string {
  const named = process.env.MNEMEX_VAULT;
  return resolve(named ? named : join(homedir(), "mnemex", "vault"));
}

function inputPath(input: unknown): string | undefined {
  if (!input || typeof input !== "object") return undefined;
  const map = input as Record<string, unknown>;
  const path = map.path ?? map.file_path ?? map.filePath;
  return typeof path === "string" ? path : undefined;
}

// The file Pi's write and edit tools actually wrote: they strip a leading `@`,
// expand `~`, accept a `file://` URL, and resolve a relative path against Pi's
// working directory rather than this process's.
function writtenPath(path: string, cwd: string): string {
  let normalized = path.replace(UNICODE_SPACES, " ");
  if (normalized.startsWith("@")) normalized = normalized.slice(1);
  if (normalized === "~") normalized = homedir();
  else if (normalized.startsWith("~/")) normalized = join(homedir(), normalized.slice(2));
  else if (normalized.startsWith("file://")) normalized = fileURLToPath(normalized);
  return isAbsolute(normalized) ? resolve(normalized) : resolve(cwd, normalized);
}

function isWithin(root: string, absolute: string): boolean {
  const rel = relative(root, absolute);
  return rel === "" || (!rel.startsWith("..") && !isAbsolute(rel));
}

function runMnemex(args: string[], stdin = "", signal?: AbortSignal): Promise<{ code: number; out: string; err: string }> {
  return new Promise((resolvePromise) => {
    const child = spawn(MNEMEX, args, { stdio: ["pipe", "pipe", "pipe"], signal });
    let out = "";
    let err = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => { out += chunk; });
    child.stderr.on("data", (chunk) => { err += chunk; });
    // A child that exits without reading its input fails the write with EPIPE;
    // unheard, that error would crash Pi itself.
    child.stdin.on("error", (error) => { err += `${error.message}\n`; });
    child.on("error", (error) => {
      resolvePromise({ code: 2, out, err: `${err}${error.message}` });
    });
    child.on("close", (code) => {
      resolvePromise({ code: code ?? 2, out, err });
    });
    child.stdin.end(stdin);
  });
}

function notifyWarning(ctx: ExtensionContext, message: string): void {
  ctx.ui.notify(message, "warning");
}

// The `check --json` envelope, as much of it as the sweep reads.
type CheckEnvelope = {
  checked?: number;
  diagnostics?: {
    path?: string;
    code?: string;
    severity?: string;
    message?: string;
    span?: { line?: number; column?: number };
  }[];
  summary?: { error?: number; warning?: number; info?: number };
};

// The findings the sweep should report, or nothing when the vault is clean.
//
// `check` exits 0 when it finds only warnings, so deciding from the exit code
// alone makes every warning invisible — and a write made outside Pi is exactly
// the case no per-write hook saw, which is the gap this sweep exists to close.
function findings(envelope: CheckEnvelope):
  | { errors: number; warnings: number; text: string }
  | undefined {
  const errors = envelope.summary?.error ?? 0;
  const warnings = envelope.summary?.warning ?? 0;
  const infos = envelope.summary?.info ?? 0;
  if (errors + warnings + infos === 0) return undefined;

  const text = (envelope.diagnostics ?? [])
    .map((d) => {
      const where = d.span
        ? `${d.path}:${d.span.line}:${d.span.column}`
        : d.path;
      return `${where}: ${d.severity}[${d.code}]: ${d.message}`;
    })
    .join("\n");
  return { errors, warnings, text };
}

export default function mnemexHooks(pi: ExtensionAPI) {
  // What the sweep last showed, so an unchanged vault stays silent. Empty is a
  // clean vault, and a new session starts there: one that starts clean says
  // nothing, and one that starts dirty says so once.
  let shown = "";

  pi.on("tool_result", async (event, ctx) => {
    if (!WRITE_TOOLS.has(event.toolName) || event.isError) return;

    const input = inputPath(event.input);
    if (!input) return;

    const root = vaultRoot();
    const path = writtenPath(input, ctx.cwd);
    if (!isWithin(root, path)) return;

    const payload = JSON.stringify({ toolName: event.toolName, input: { path } });
    const hook = await runMnemex(["hook"], payload, ctx.signal);
    if (hook.code !== 0) {
      notifyWarning(ctx, `mnemex hook exited ${hook.code}`);
    }
    if (hook.err.trim()) {
      notifyWarning(ctx, hook.err.trim());
    }
    if (!hook.out.trim()) return;

    let response: HookResponse;
    try {
      response = JSON.parse(hook.out) as HookResponse;
    } catch {
      notifyWarning(ctx, "mnemex hook returned invalid JSON");
      return;
    }

    if (response.decision === "block") {
      return {
        isError: true,
        content: [{ type: "text", text: response.reason ?? "mnemex rejected this completed write" }],
      };
    }

    const warning = response.hookSpecificOutput?.additionalContext;
    if (warning) {
      return {
        content: [
          ...event.content,
          { type: "text", text: `mnemex check reported warnings:\n\n${warning}` },
        ],
      };
    }
  });

  // Every settle sweeps, not only one after a write through Pi's tools: a shell
  // write, an editor, or another session changes the vault without any tool
  // event, and those are the writes the per-write hook never sees. A sweep
  // costs milliseconds; what makes it quiet is showing only what changed.
  pi.on("agent_settled", async (_event, ctx) => {
    const root = vaultRoot();
    if (!existsSync(root)) return;

    const check = await runMnemex(["check", "--json"], "", ctx.signal);

    let envelope: CheckEnvelope | undefined;
    try {
      envelope = JSON.parse(check.out) as CheckEnvelope;
    } catch {
      envelope = undefined;
    }

    let message = "";
    let level: "error" | "warning" | "info" = "info";
    if (!envelope) {
      // An exit-0 sweep with no envelope is not a failure: only a refusal is.
      if (check.code !== 0) {
        message = check.out || check.err || "mnemex check failed";
        level = "error";
      }
    } else {
      const found = findings(envelope);
      if (found) {
        const trailer = `${found.errors} error(s), ${found.warnings} warning(s) in ${envelope.checked ?? 0} note(s)`;
        message = `${trailer}\n\n${found.text}`;
        level = found.errors > 0 ? "error" : "warning";
      }
    }

    if (message === shown) return;
    const recovered = message === "";
    shown = message;
    ctx.ui.notify(recovered ? "mnemex check: the vault is clean again" : message, recovered ? "info" : level);
  });
}
