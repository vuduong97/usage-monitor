// node --test integrations/pi/clawd-home.test.ts   (Node >= 23: types stripped natively)
import { test } from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import clawdHome, { post, sessionId } from "./clawd-home.ts";

function fakePi() {
  const handlers = new Map<string, (e: any, ctx: any) => unknown>();
  return { handlers, pi: { on: (name: string, h: (e: any, ctx: any) => unknown) => { handlers.set(name, h); } } };
}
const tui = { mode: "tui", sessionManager: { getSessionId: () => "S1" } };
const child = { mode: "print", sessionManager: { getSessionId: () => "C1" } };

test("maps pi lifecycle onto clawd events and never returns a value", () => {
  const sent: any[] = [];
  const { pi, handlers } = fakePi();
  clawdHome(pi, (b) => sent.push(b));
  const fire = (name: string, e: any = {}, ctx: any = tui) => handlers.get(name)!(e, ctx);
  for (const [name, e] of [
    ["before_agent_start", {}],
    ["tool_call", { toolName: "bash", input: { command: "ls" } }],
    ["tool_result", { toolName: "bash", isError: true }],
    ["tool_call", { toolName: "subagent" }],
    ["tool_result", { toolName: "subagent" }],
    ["tool_call", { toolName: "ask_user_question", input: { questions: [{ question: "Which DB?" }] } }],
    ["ui_prompt_start", { title: "Allow?" }],
    ["ui_prompt_end", {}],
    ["session_before_compact", {}],
    ["session_compact", {}],
    ["agent_settled", { aborted: false }],
    ["agent_settled", { aborted: true }],
    ["session_shutdown", {}],
  ] as const) assert.equal(fire(name, e), undefined);
  assert.deepEqual(sent.map((s) => s.ev), [
    "prompt", "tool", "tool_error", "tool", "subagent_start", "subagent_stop", "tool_done",
    "attention", "attention", "attention_done", "compact", "compact_done", "stop", "abort", "end",
  ]);
  assert.equal(sent[7].msg, "Which DB?");
  assert.ok(sent.every((s) => s.sid === "S1" && s.sub === false));
  fire("before_agent_start", {}, child);
  assert.deepEqual(sent.at(-1), { ev: "prompt", sid: "C1", sub: true });
});

test("a throwing sender or session manager never escapes", () => {
  const { pi, handlers } = fakePi();
  clawdHome(pi, () => { throw new Error("boom"); });
  assert.equal(handlers.get("tool_call")!({ toolName: "x" }, { sessionManager: { getSessionId: () => { throw new Error(); } } }), undefined);
  assert.equal(sessionId({ sessionManager: { getSessionId: () => { throw new Error(); } } }), "default");
});

test("post() delivers JSON to /hook/pi and is silent when nothing listens", async () => {
  const got: any[] = [];
  const srv = http.createServer((req, res) => {
    let b = "";
    req.on("data", (c) => (b += c));
    req.on("end", () => { got.push({ url: req.url, body: JSON.parse(b) }); res.end("{}"); });
  });
  await new Promise<void>((r) => srv.listen(0, "127.0.0.1", () => r()));
  const port = (srv.address() as any).port;
  post({ ev: "prompt", sid: "x" }, port);
  await new Promise((r) => setTimeout(r, 200));
  assert.deepEqual(got, [{ url: "/hook/pi", body: { ev: "prompt", sid: "x" } }]);
  srv.close();
  post({ ev: "prompt", sid: "x" }, port);       // refused: must not throw
  await new Promise((r) => setTimeout(r, 100));
});
