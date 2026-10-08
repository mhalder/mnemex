// The Pi extension's path handling, driven through a fake `pi` and a stub
// `mnemex` that logs each call's arguments and stdin.

import assert from "node:assert/strict";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, beforeEach, test } from "node:test";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

const scratch = mkdtempSync(join(tmpdir(), "memex-hooks-"));
const log = join(scratch, "calls.log");
const reply = join(scratch, "reply");
// When this file exists, the stub prints it to stderr and exits 2, as a
// refused verb does.
const refusal = join(scratch, "refusal");
// When this file exists, the stub closes its stdin unread and exits.
const closesStdin = join(scratch, "closes-stdin");
const stub = join(scratch, "mnemex");
writeFileSync(
  stub,
  `#!/bin/sh
if [ -f '${closesStdin}' ]; then exec 0<&-; exit 0; fi
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

// The extension reads MNEMEX when it loads and MNEMEX_VAULT on every event.
process.env.MNEMEX = stub;
process.env.MNEMEX_VAULT = vault;
const { default: mnemexHooks } = await import("../../extensions/pi.ts");

type Handler = (event: unknown, ctx: unknown) => Promise<unknown>;

/// Load the extension against a fake `pi`, with `cwd` as Pi's working directory.
function load(cwd = scratch) {
  const handlers = new Map<string, Handler>();
  const notes: [string, string][] = [];
  const pi = { on: (name: string, handler: Handler) => handlers.set(name, handler) };
  mnemexHooks(pi as unknown as ExtensionAPI);
  const ctx = {
    cwd,
    signal: undefined,
    ui: { notify: (message: string, kind: string) => notes.push([kind, message]) },
  };
  const handler = (name: string): Handler => {
    const found = handlers.get(name);
    assert.ok(found, `the extension registers ${name}`);
    return found;
  };
  return {
    notes,
    toolResult: (toolName: string, input: Record<string, unknown>) =>
      handler("tool_result")(
        { type: "tool_result", toolName, input, content: [{ type: "text", text: "wrote" }], isError: false },
        ctx,
      ),
    settled: () => handler("agent_settled")({ type: "agent_settled" }, ctx),
  };
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

/// The paths the hook was sent, in call order.
function hookedPaths(): string[] {
  return calls()
    .filter((c) => c.args === "hook")
    .map((c) => JSON.parse(c.stdin).input.path);
}

beforeEach(() => {
  rmSync(log, { force: true });
  rmSync(reply, { force: true });
});

after(() => {
  rmSync(scratch, { recursive: true, force: true });
});

test("a write inside the vault is sent to the hook as a Pi payload", async () => {
  const result = await load().toolResult("write", { path: inside });
  assert.equal(result, undefined);
  assert.deepEqual(calls(), [{ args: "hook", stdin: JSON.stringify({ toolName: "write", input: { path: inside } }) }]);
});

test("the written path may be named path, file_path, or filePath", async () => {
  for (const key of ["path", "file_path", "filePath"]) {
    rmSync(log, { force: true });
    await load().toolResult("edit", { [key]: inside });
    assert.deepEqual(hookedPaths(), [inside], key);
  }
});

test("a relative path is resolved against Pi's working directory, not the process's", async () => {
  await load(vault).toolResult("write", { path: "notes/202609081100-n.md" });
  assert.deepEqual(hookedPaths(), [inside]);
});

test("an `@`-prefixed path is the path after the `@`, as Pi's tools read it", async () => {
  await load().toolResult("edit", { path: `@${inside}` });
  assert.deepEqual(hookedPaths(), [inside]);
});

test("a `~/` path is expanded from the home directory, as Pi's tools read it", async () => {
  const home = process.env.HOME;
  process.env.HOME = scratch;
  try {
    await load().toolResult("write", { path: "~/vault/notes/202609081100-n.md" });
  } finally {
    process.env.HOME = home;
  }
  assert.deepEqual(hookedPaths(), [inside]);
});

test("an empty MNEMEX_VAULT counts as unset, as mnemex reads it", async () => {
  const saved = { vault: process.env.MNEMEX_VAULT, home: process.env.HOME };
  const home = join(scratch, "home");
  const note = join(home, "mnemex", "vault", "notes", "202609081100-n.md");
  mkdirSync(join(home, "mnemex", "vault", "notes"), { recursive: true });
  process.env.MNEMEX_VAULT = "";
  process.env.HOME = home;
  try {
    await load().toolResult("write", { path: note });
  } finally {
    process.env.MNEMEX_VAULT = saved.vault;
    process.env.HOME = saved.home;
  }
  assert.deepEqual(hookedPaths(), [note]);
});

test("writes outside the vault, escaping it, unnamed, or by other tools never reach the hook", async () => {
  const h = load();
  await h.toolResult("write", { path: join(scratch, "elsewhere.md") });
  await h.toolResult("write", { path: join(vault, "..", "escape.md") });
  await h.toolResult("write", { path: 3 });
  await h.toolResult("write", {});
  await h.toolResult("read", { path: inside });
  assert.deepEqual(calls(), []);
});

test("a mnemex that closes its stdin unread does not crash Pi", async () => {
  writeFileSync(closesStdin, "");
  try {
    // Far more than a pipe buffer holds, so writing the payload fails.
    const long = join(vault, "notes", `${"x".repeat(1_000_000)}.md`);
    assert.equal(await load().toolResult("write", { path: long }), undefined);
  } finally {
    rmSync(closesStdin, { force: true });
  }
});

test("a blocking hook reply fails the tool result with its reason", async () => {
  writeFileSync(reply, JSON.stringify({ decision: "block", reason: "error[MX102]" }));
  const result = await load().toolResult("write", { path: inside });
  assert.deepEqual(result, { isError: true, content: [{ type: "text", text: "error[MX102]" }] });
});

test("hook warnings are appended to the tool's own content", async () => {
  writeFileSync(reply, JSON.stringify({ hookSpecificOutput: { additionalContext: "warning[MX105]" } }));
  const result = await load().toolResult("write", { path: inside });
  assert.deepEqual(result, {
    content: [
      { type: "text", text: "wrote" },
      { type: "text", text: "mnemex check reported warnings:\n\nwarning[MX105]" },
    ],
  });
});

test("a hook reply that is not JSON is a warning, not a result", async () => {
  writeFileSync(reply, "not json");
  const h = load();
  assert.equal(await h.toolResult("write", { path: inside }), undefined);
  assert.deepEqual(h.notes, [["warning", "mnemex hook returned invalid JSON"]]);
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

test("every settle sweeps the vault, with or without a write through Pi's tools", async () => {
  const h = load();
  await h.settled();
  await h.settled();
  assert.deepEqual(calls(), [
    { args: "check --json", stdin: "" },
    { args: "check --json", stdin: "" },
  ]);
});

test("unchanged findings are shown once, and a vault that turns clean says so once", async () => {
  const h = load();
  writeFileSync(reply, oneWarning);
  await h.settled();
  await h.settled();
  assert.equal(h.notes.length, 1);

  writeFileSync(reply, clean);
  await h.settled();
  await h.settled();
  assert.deepEqual(h.notes.slice(1), [["info", "mnemex check: the vault is clean again"]]);

  writeFileSync(reply, oneWarning);
  await h.settled();
  assert.equal(h.notes.length, 3);
});

test("each session shows standing findings once", async () => {
  writeFileSync(reply, oneWarning);
  const first = load();
  await first.settled();
  const second = load();
  await second.settled();
  assert.equal(first.notes.length, 1);
  assert.equal(second.notes.length, 1);
});

test("a clean sweep stays silent", async () => {
  const h = load();
  await h.toolResult("write", { path: inside });
  writeFileSync(
    reply,
    JSON.stringify({ version: 1, checked: 2, diagnostics: [], summary: { error: 0, warning: 0, info: 0 } }),
  );
  await h.settled();
  assert.deepEqual(h.notes, []);
});

test("a sweep that finds only warnings reports them instead of staying silent", async () => {
  const h = load();
  await h.toolResult("write", { path: inside });
  // Exit 0 with warnings present: the case the sweep exists for, and the one
  // an exit-code test alone cannot see.
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
  await h.settled();
  assert.deepEqual(h.notes, [
    [
      "warning",
      "0 error(s), 1 warning(s) in 2 note(s)\n\n" +
        "/v/notes/202609081100-n.md:3:7: warning[MX405]: `repo` is not set",
    ],
  ]);
});

test("a sweep that finds errors reports them as errors", async () => {
  const h = load();
  await h.toolResult("write", { path: inside });
  writeFileSync(
    reply,
    JSON.stringify({
      version: 1,
      checked: 1,
      diagnostics: [
        {
          path: "/v/notes/202609081100-n.md",
          code: "MX202",
          severity: "error",
          message: "`project` points at `nope`",
          span: { line: 2, column: 10 },
        },
      ],
      summary: { error: 1, warning: 0, info: 0 },
    }),
  );
  await h.settled();
  assert.deepEqual(h.notes, [
    [
      "error",
      "1 error(s), 0 warning(s) in 1 note(s)\n\n" +
        "/v/notes/202609081100-n.md:2:10: error[MX202]: `project` points at `nope`",
    ],
  ]);
});

test("a settle sweep that mnemex refuses is reported as an error, not silence", async () => {
  const h = load();
  await h.toolResult("write", { path: inside });
  writeFileSync(refusal, "error: `$MNEMEX_VAULT` names /x, which holds none of the governed folders\n");
  try {
    await h.settled();
  } finally {
    rmSync(refusal, { force: true });
  }
  assert.deepEqual(h.notes, [
    ["error", "error: `$MNEMEX_VAULT` names /x, which holds none of the governed folders\n"],
  ]);
});
