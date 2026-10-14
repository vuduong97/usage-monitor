// node --test integrations/opencode/clawd.test.mjs
import assert from "node:assert/strict";
import http from "node:http";
import test from "node:test";
import * as mod from "./clawd.js";

const { ClawdPlugin } = mod;

function collect() {
  const got = [];
  return { got, hooks: ClawdPlugin.makeHooks((b) => got.push(b)) };
}
const event = (type, properties) => ({ event: { type, properties } });

test("only the plugin itself is exported (opencode calls every exported function)", () => {
  assert.deepEqual(Object.keys(mod), ["ClawdPlugin"]);
});

test("a turn: prompt once per user message, tools, then stop on idle", async () => {
  const { got, hooks } = collect();
  await hooks.event(event("message.updated", { info: { id: "m1", role: "user", sessionID: "s" } }));
  await hooks.event(event("message.updated", { info: { id: "m1", role: "user", sessionID: "s" } }));
  await hooks.event(event("session.status", { sessionID: "s", status: { type: "busy" } }));
  await hooks["tool.execute.before"]({ tool: "bash", sessionID: "s", callID: "c" }, { args: {} });
  await hooks["tool.execute.after"]({ tool: "bash", sessionID: "s", callID: "c" }, {});
  await hooks.event(event("session.idle", { sessionID: "s" }));
  await hooks.event(event("session.idle", { sessionID: "s" }));
  assert.deepEqual(got.map((b) => b.ev), ["prompt", "tool", "tool_done", "stop"]);
  assert.equal(got[1].tool, "bash");
});

test("subagents, questions, permissions and errors", async () => {
  const { got, hooks } = collect();
  await hooks.event(event("session.created", { info: { id: "child", parentID: "s" } }));
  await hooks["tool.execute.before"]({ tool: "task", sessionID: "s" });
  await hooks["tool.execute.before"]({ tool: "question", sessionID: "s" });
  await hooks.event(event("permission.asked", { id: "p", sessionID: "s", action: "bash" }));
  await hooks.event(event("permission.replied", { sessionID: "s", requestID: "p", reply: "once" }));
  await hooks.event(event("session.status", { sessionID: "child", status: { type: "busy" } }));
  await hooks.event(event("session.error", { sessionID: "s", error: { name: "MessageAbortedError" } }));
  assert.deepEqual(got.map((b) => [b.ev, b.sub]), [
    ["start", true], ["subagent_start", false], ["tool", false], ["attention", false],
    ["attention", false], ["attention_done", false], ["prompt", true], ["abort", false],
  ]);
  assert.equal(got[4].msg, "Allow bash");
});

test("a malformed event never throws", async () => {
  const { got, hooks } = collect();
  await hooks.event(undefined);
  await hooks.event({ event: null });
  await hooks["tool.execute.before"](undefined);
  assert.equal(got.length, 0);           // no session id: nothing to report
});

test("post() delivers JSON to /hook/opencode and is silent when nothing listens", async () => {
  const seen = [];
  const srv = http.createServer((req, res) => {
    let b = "";
    req.on("data", (c) => (b += c));
    req.on("end", () => { seen.push({ url: req.url, body: JSON.parse(b) }); res.end("{}"); });
  });
  await new Promise((r) => srv.listen(0, "127.0.0.1", r));
  ClawdPlugin.post({ ev: "prompt", sid: "x" }, srv.address().port);
  await new Promise((r) => setTimeout(r, 200));
  srv.close();
  assert.deepEqual(seen, [{ url: "/hook/opencode", body: { ev: "prompt", sid: "x" } }]);
  ClawdPlugin.post({ ev: "prompt", sid: "x" }, 1);       // refused: no throw
});
