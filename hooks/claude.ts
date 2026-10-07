// The Claude Code counterpart of extensions/pi.ts: one command hook that reads a
// hook event on stdin and dispatches on its name.
//
//   PostToolUse (Write|Edit)  a write inside the vault is sent to `mnemex hook`,
//                             whose reply is already a Claude hook envelope.
//   Stop                      sweep the vault with `mnemex check --json` and
//                             show the findings, when they changed.
//
// Each hook is its own process, so what the sweep last showed, which Pi keeps
// in memory, is a file per session here.

import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";

const MNEMEX = process.env.MNEMEX ?? "mnemex";
const WRITE_TOOLS = new Set(["Write", "Edit"]);

type HookEvent = {
  hook_event_name?: string;
  session_id?: string;
  cwd?: string;
  tool_name?: string;
  tool_input?: Record<string, unknown>;
};

type Output = Record<string, unknown> & { systemMessage?: string };

// An empty MNEMEX_VAULT counts as unset, as mnemex reads it.
function vaultRoot(): string {
  const named = process.env.MNEMEX_VAULT;
  return resolve(named ? named : join(homedir(), "mnemex-vault"));
}

function isWithin(root: string, absolute: string): boolean {
  const rel = relative(root, absolute);
  return rel === "" || (!rel.startsWith("..") && !isAbsolute(rel));
}

// Where a session's last shown sweep is recorded. The session id comes from
// Claude, but is still reduced to a file name so it cannot name a path.
function shownFile(session: string): string {
  const dir = join(process.env.XDG_RUNTIME_DIR || tmpdir(), "mnemex-claude-hooks");
  return join(dir, session.replace(/[^A-Za-z0-9_-]/g, "_"));
}

function runMnemex(args: string[], stdin = ""): Promise<{ code: number; out: string; err: string }> {
  return new Promise((resolvePromise) => {
    const child = spawn(MNEMEX, args, { stdio: ["pipe", "pipe", "pipe"] });
    let out = "";
    let err = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => { out += chunk; });
    child.stderr.on("data", (chunk) => { err += chunk; });
    // A child that exits without reading its input fails the write with EPIPE.
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

function note(output: Output, message: string): void {
  output.systemMessage = output.systemMessage ? `${output.systemMessage}\n${message}` : message;
}

async function postToolUse(event: HookEvent, payload: string): Promise<Output | undefined> {
  if (!WRITE_TOOLS.has(event.tool_name ?? "")) return undefined;
  const input = event.tool_input?.file_path;
  if (typeof input !== "string") return undefined;

  const path = resolve(event.cwd ?? process.cwd(), input);
  if (!isWithin(vaultRoot(), path)) return undefined;

  const hook = await runMnemex(["hook"], payload);
  let output: Output = {};
  if (hook.out.trim()) {
    try {
      const parsed: unknown = JSON.parse(hook.out);
      if (parsed && typeof parsed === "object") output = parsed as Output;
      else note(output, "mnemex hook returned invalid JSON");
    } catch {
      note(output, "mnemex hook returned invalid JSON");
    }
  }
  if (hook.code !== 0) note(output, `mnemex hook exited ${hook.code}`);
  if (hook.err.trim()) note(output, hook.err.trim());
  return Object.keys(output).length > 0 ? output : undefined;
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

// Every turn sweeps, not only one that wrote through Write or Edit: a shell
// write, an editor, or another session changes the vault without any tool
// event. A sweep costs milliseconds; what keeps it quiet is showing only what
// changed since this session last showed anything. Nothing shown yet counts as
// a clean vault, so a session that starts clean says nothing.
//
// `check` exits 0 when it finds only warnings, so the envelope decides, not the
// exit code; see the same function in extensions/pi.ts.
async function stop(event: HookEvent): Promise<Output | undefined> {
  if (!event.session_id) return undefined;
  if (!existsSync(vaultRoot())) return undefined;

  const check = await runMnemex(["check", "--json"]);
  let envelope: CheckEnvelope | undefined;
  try {
    envelope = JSON.parse(check.out) as CheckEnvelope;
  } catch {
    envelope = undefined;
  }

  let message = "";
  if (!envelope) {
    // An exit-0 sweep with no envelope is not a failure: only a refusal is.
    if (check.code !== 0) message = (check.out || check.err || "mnemex check failed").trimEnd();
  } else {
    const errors = envelope.summary?.error ?? 0;
    const warnings = envelope.summary?.warning ?? 0;
    const infos = envelope.summary?.info ?? 0;
    if (errors + warnings + infos > 0) {
      const text = (envelope.diagnostics ?? [])
        .map((d) => {
          const where = d.span ? `${d.path}:${d.span.line}:${d.span.column}` : d.path;
          return `${where}: ${d.severity}[${d.code}]: ${d.message}`;
        })
        .join("\n");
      const trailer = `${errors} error(s), ${warnings} warning(s) in ${envelope.checked ?? 0} note(s)`;
      message = `mnemex check: ${trailer}\n\n${text}`;
    }
  }

  const file = shownFile(event.session_id);
  const digest = message ? createHash("sha256").update(message).digest("hex") : "";
  let shown = "";
  try {
    shown = readFileSync(file, "utf8");
  } catch {
    shown = "";
  }
  if (digest === shown) return undefined;
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, digest);
  return { systemMessage: message || "mnemex check: the vault is clean again" };
}

async function readStdin(): Promise<string> {
  let data = "";
  process.stdin.setEncoding("utf8");
  for await (const chunk of process.stdin) data += chunk;
  return data;
}

// Always exits 0: a hook failure must never fail the tool call it reports on.
const payload = await readStdin();
let event: HookEvent = {};
try {
  event = JSON.parse(payload) as HookEvent;
} catch {
  process.exit(0);
}
const output =
  event.hook_event_name === "PostToolUse" ? await postToolUse(event, payload)
  : event.hook_event_name === "Stop" ? await stop(event)
  : undefined;
if (output) process.stdout.write(`${JSON.stringify(output)}\n`);
