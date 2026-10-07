// The Claude Code hook, run as Claude runs it: one process per event, the event
// on stdin, against a stub `mnemex` that logs each call's arguments and stdin.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { after, beforeEach, test } from "node:test";

const HOOK = resolve(import.meta.dirname, "../../hooks/claude.ts");

const scratch = mkdtempSync(join(tmpdir(), "mnemex-claude-hooks-"));
const log = join(scratch, "calls.log");
const reply = join(scratch, "reply");
// When this file exists, the stub prints it to stderr and exits 2, as a
// refused verb does.
const refusal = join(scratch, "refusal");
const stub = join(scratch, "mnemex");
writeFileSync(
  stub,
  `#!/bin/sh
input=$(cat)
printf '%s\\t%s\\n' "$*" "$input" >> '${log}'
if [ -f '${refusal}' ]; then cat '${refusal}' >&2; exit 2; fi
if [ -f '${reply}' ]; then cat '${reply}'; fi
exit 0
`,
);
chmodSync(stub, 0o755);

const vault = join(scratch, "vault");
mkdirSync(join(vault, "notes"), { recursive: true });
const inside = join(vault, "notes", "202609081100-n.md");
const runtime = join(scratch, "run");
mkdirSync(runtime);

/// Run the hook on one event; the parsed stdout, or undefined when silent.
function run(event: Record<string, unknown>): Record<string, unknown> | undefined {
  const result = spawnSync(process.execPath, [HOOK], {
    input: JSON.stringify(event),
    encoding: "utf8",
    env: { ...process.env, MNEMEX: stub, MEMEX_VAULT: vault, XDG_RUNTIME_DIR: runtime },
  });
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.trim() ? JSON.parse(result.stdout) : undefined;
}

function write(file: string, tool = "Write", session = "s1") {
  return run({ hook_event_name: "PostToolUse", session_id: session, cwd: scratch, tool_name: tool, tool_input: { file_path: file } });
}

function stop(session = "s1") {
  return run({ hook_event_name: "Stop", session_id: session, cwd: scratch });
}

function calls(): { args: string; stdin: string }[] {
  if (!existsSync(log)) return [];
  return readFileSync(log, "utf8")
    .trimEnd()
    .split("\n")
    .map((line) => {
      const [args = "", stdin = ""] = line.split("\t");
      return { args, stdin };
    });
}

beforeEach(() => {
  rmSync(log, { force: true });
  rmSync(reply, { force: true });
  rmSync(join(runtime, "mnemex-claude-hooks"), { recursive: true, force: true });
});

after(() => {
  rmSync(scratch, { recursive: true, force: true });
});

test("a Write or Edit inside the vault is sent to the hook as Claude's own payload", () => {
  for (const tool of ["Write", "Edit"]) {
    rmSync(log, { force: true });
    assert.equal(write(inside, tool), undefined);
    const [call, ...rest] = calls();
    assert.equal(call?.args, "hook", tool);
    assert.equal(JSON.parse(call?.stdin ?? "").tool_input.file_path, inside, tool);
    assert.deepEqual(rest, []);
  }
});

test("writes outside the vault, escaping it, unnamed, or by other tools never reach the hook", () => {
  write(join(scratch, "elsewhere.md"));
  write(join(vault, "..", "escape.md"));
  run({ hook_event_name: "PostToolUse", session_id: "s1", tool_name: "Write", tool_input: {} });
  write(inside, "Read");
  assert.deepEqual(calls(), []);
});

test("the hook's reply is passed through as the hook's output", () => {
  const block = { decision: "block", reason: "error[MX102]" };
  writeFileSync(reply, JSON.stringify(block));
  assert.deepEqual(write(inside), block);
});

test("a hook reply that is not JSON is shown to the user, not passed through", () => {
  writeFileSync(reply, "not json");
  assert.deepEqual(write(inside), { systemMessage: "mnemex hook returned invalid JSON" });
});

const clean = JSON.stringify({ version: 1, checked: 2, diagnostics: [], summary: { error: 0, warning: 0, info: 0 } });
const oneWarning = JSON.stringify({
  version: 1,
  checked: 2,
  diagnostics: [
    { path: "/v/notes/202609081100-n.md", code: "MX405", severity: "warning", message: "`repo` is not set", span: { line: 3, column: 7 } },
  ],
  summary: { error: 0, warning: 1, info: 0 },
});

test("every Stop sweeps the vault, with or without a Write or Edit", () => {
  stop();
  stop();
  assert.deepEqual(calls(), [
    { args: "check --json", stdin: "" },
    { args: "check --json", stdin: "" },
  ]);
});

test("unchanged findings are shown once, and a vault that turns clean says so once", () => {
  writeFileSync(reply, oneWarning);
  assert.ok(stop()?.systemMessage);
  assert.equal(stop(), undefined);

  writeFileSync(reply, clean);
  assert.deepEqual(stop(), { systemMessage: "mnemex check: the vault is clean again" });
  assert.equal(stop(), undefined);

  writeFileSync(reply, oneWarning);
  assert.ok(stop()?.systemMessage);
});

test("each session shows standing findings once", () => {
  writeFileSync(reply, oneWarning);
  assert.ok(stop("s1")?.systemMessage);
  assert.ok(stop("s2")?.systemMessage);
  assert.equal(stop("s1"), undefined);
});

test("a clean sweep stays silent", () => {
  writeFileSync(reply, clean);
  assert.equal(stop(), undefined);
});

test("a sweep that finds only warnings reports them instead of staying silent", () => {
  writeFileSync(
    reply,
    JSON.stringify({
      version: 1,
      checked: 2,
      diagnostics: [
        {
          path: "/v/notes/202609081100-n.md",
          code: "MX405",
          severity: "warning",
          message: "`repo` is not set",
          span: { line: 3, column: 7 },
        },
      ],
      summary: { error: 0, warning: 1, info: 0 },
    }),
  );
  assert.deepEqual(stop(), {
    systemMessage:
      "mnemex check: 0 error(s), 1 warning(s) in 2 note(s)\n\n" +
      "/v/notes/202609081100-n.md:3:7: warning[MX405]: `repo` is not set",
  });
});

test("a Stop sweep that mnemex refuses is reported, not silence", () => {
  writeFileSync(refusal, "error: `$MEMEX_VAULT` names /x, which holds none of the governed folders\n");
  try {
    assert.deepEqual(stop(), {
      systemMessage: "error: `$MEMEX_VAULT` names /x, which holds none of the governed folders",
    });
  } finally {
    rmSync(refusal, { force: true });
  }
});

test("an event that is not JSON is ignored", () => {
  const result = spawnSync(process.execPath, [HOOK], { input: "nope", encoding: "utf8" });
  assert.equal(result.status, 0);
  assert.equal(result.stdout, "");
});
