// node --test integrations/pi/clawd-home.test.ts   (Node >= 23: types stripped natively)
import { test } from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import { EventEmitter } from "node:events";
import clawdHome, {
  isDebugPrompt, isDebugSkillRead, isUserInput, post, promptSkill, sessionId, skillRead,
} from "./clawd-home.ts";

function fakePi() {
  const handlers = new Map<string, (e: any, ctx: any) => unknown>();
  return { handlers, pi: { on: (name: string, h: (e: any, ctx: any) => unknown) => { handlers.set(name, h); } } };
}
const tui = { mode: "tui", sessionManager: { getSessionId: () => "S1" } };
const child = { mode: "print", sessionManager: { getSessionId: () => "C1" } };

test("maps pi lifecycle onto clawd events and never returns a value", () => {
  const sent: any[] = [];
  const { pi, handlers } = fakePi();
  clawdHome(pi, (b) => sent.push(b), fakeStdin());
  const fire = (name: string, e: any = {}, ctx: any = tui) => handlers.get(name)!(e, ctx);
  for (const [name, e] of [
    ["before_agent_start", {}],
    ["tool_call", { toolName: "bash", input: { command: "ls" } }],
    ["tool_result", { toolName: "bash", isError: true }],
    ["tool_call", { toolName: "subagent" }],
    ["tool_result", { toolName: "subagent" }],
    ["tool_call", { toolName: "ask_user_question", input: { questions: [{ question: "Which DB?" }] } }],
    ["tool_result", { toolName: "ask_user_question" }],
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
    "attention", "tool_done", "attention", "attention_done", "compact", "compact_done", "stop", "abort", "end",
  ]);
  assert.equal(sent[7].tool, "ask_user_question");
  assert.ok(!JSON.stringify(sent).includes("Which DB?"));   // the question never leaves pi
  assert.equal(sent[1].cmd, "ls");               // bash commands go along (test runs)
  assert.equal(sent[3].cmd, undefined);
  assert.ok(sent.every((s) => s.sid === "S1" && s.sub === false));
  fire("before_agent_start", {}, child);
  assert.deepEqual(sent.at(-1), { ev: "prompt", sid: "C1", sub: true });
});

test("isUserInput counts focus-in, clicks and keys but not replies, motion or wheel", () => {
  const yes = [
    "\x1b[I",                       // focus in
    "\x1b[<0;12;5M",                // left press
    "\x1b[<2;1;1M",                 // right press
    "\x1b[<16;3;4M",                // ctrl+left press
    "a", "\r", "\x7f", "\x1b", "\x1bx", "y",
    "\x1b[A", "\x1b[1;5C", "\x1b[3~", "\x1b[Z", "\x1bOP", "\x1b[13u", "\x1b[97;5u",
    "\x1b[200~pasted\x1b[201~",
    "\x1b[O\x1b[I",                 // out + in in one chunk
    "\x1b]4;1;rgb:ff/00/00\x07q",   // a reply followed by a key
    Buffer.from("\x1b[I"),
  ];
  const no = [
    "", "\x1b[O",                   // focus out
    "\x1b[<0;12;5m",                // release
    "\x1b[<35;12;5M",               // motion (hover)
    "\x1b[<32;12;5M",               // drag
    "\x1b[<64;12;5M", "\x1b[<65;1;1M", // wheel
    "\x1b[6;18;9t",                 // cell size
    "\x1b[?62;22c",                 // device attributes
    "\x1b[?997;1n",                 // color scheme change
    "\x1b[?1u",                     // kitty keyboard flags
    "\x1b[?2026;2$y",               // mode report
    "\x1b[12;40R",                  // cursor position
    "\x1b]11;rgb:1e1e/1e1e/1e1e\x1b\\", // OSC reply, ST terminator
    "\x1b]4;0;rgb:00/00/00\x07\x1b]4;1;rgb:cd/00/00\x07",
    "\x1b_Gi=1;OK\x1b\\",           // kitty graphics reply
    undefined, 42, {},
  ];
  for (const c of yes) assert.equal(isUserInput(c), true, JSON.stringify(String(c)));
  for (const c of no) assert.equal(isUserInput(c), false, JSON.stringify(String(c)));
});

function fakeStdin(isTTY = true) {
  const em = new EventEmitter() as EventEmitter & { isTTY?: boolean };
  em.isTTY = isTTY;
  return em;
}

test("going to a waiting session sends attention_seen once per question", () => {
  const sent: any[] = [];
  const { pi, handlers } = fakePi();
  const stdin = fakeStdin();
  clawdHome(pi, (b) => sent.push(b), stdin);
  const fire = (name: string, e: any = {}, ctx: any = tui) => handlers.get(name)!(e, ctx);
  const seen = () => sent.filter((s) => s.ev === "attention_seen");

  stdin.emit("data", "x");                          // nothing pending, not even listening
  assert.equal(stdin.listenerCount("data"), 0);
  fire("ui_prompt_start", {});
  assert.equal(stdin.listenerCount("data"), 1);
  stdin.emit("data", "\x1b[<35;1;1M");             // hover only
  stdin.emit("data", "\x1b[?997;1n");              // a terminal reply
  assert.equal(seen().length, 0);
  stdin.emit("data", Buffer.from("\x1b[I"));       // the user switched to this tab
  stdin.emit("data", "y");
  stdin.emit("data", "\x1b[<0;1;1M");
  assert.deepEqual(seen(), [{ ev: "attention_seen", sid: "S1", sub: false }]);

  fire("ui_prompt_end");
  fire("tool_call", { toolName: "ask_user_question", input: { question: "Which?" } });
  stdin.emit("data", "\r");
  assert.equal(seen().length, 2);                   // a new question is a new "seen"

  fire("tool_call", { toolName: "question" });
  fire("tool_result", { toolName: "question" });     // answered before any input reached us
  stdin.emit("data", "z");
  fire("ui_prompt_start", {});
  fire("agent_settled", { aborted: true });
  stdin.emit("data", "z");
  assert.equal(seen().length, 2);

  assert.equal(stdin.listenerCount("data"), 1);     // never stacked
  fire("session_shutdown");
  assert.equal(stdin.listenerCount("data"), 0);
});

test("debug turns send only a verdict, matching the daemon's rules", () => {
  for (const [prompt, debug] of [
    ["/ak:debug the flash script", true], ["/skill:ak-fix x", true], ["why does debugging fail", true],
    ["Fix the build", true], ["a bug here", true], ["s\u1eeda l\u1ed7i flash.sh", true],
    ["su\u031b\u0309a lo\u0302\u0303i", true],                       // NFD input
    ["tr\u1ea3 l\u1eddi b\u1eb1ng ti\u1ebfng Vi\u1ec7t", false], ["tra loi nhanh", false],
    ["add a prefix and a suffix", false], ["debounce the input", false],
    ['<skill name="ak-debug" location="x">\nbody\n</skill>\n\nwhy?', true],
    ['<skill name="ak-cook" location="x">\nfix bugs, debug\n</skill>\n\n--auto', false],
    [undefined, false],
  ] as const) assert.equal(isDebugPrompt(prompt), debug, String(prompt));
  assert.ok(isDebugSkillRead("read", { path: "/u/.agents/skills/ak-debug/SKILL.md" }));
  assert.ok(!isDebugSkillRead("read", { path: "skills/ak-debug/references/x.md" }));
  assert.ok(!isDebugSkillRead("edit", { path: "skills/ak-fix/SKILL.md" }));

  const sent: any[] = [];
  const { pi, handlers } = fakePi();
  clawdHome(pi, (b) => sent.push(b), fakeStdin());
  const fire = (name: string, e: any = {}) => handlers.get(name)!(e, tui);
  fire("before_agent_start", { prompt: "s\u1eeda l\u1ed7i flash.sh" });
  fire("before_agent_start", { prompt: "tr\u1ea3 l\u1eddi ng\u1eafn" });
  fire("tool_call", { toolName: "read", input: { path: "/s/ak-fix/SKILL.md" } });
  fire("tool_call", { toolName: "read", input: { path: "README.md" } });
  assert.deepEqual(sent, [
    { ev: "prompt", sid: "S1", sub: false, debug: true },
    { ev: "prompt", sid: "S1", sub: false },
    { ev: "tool", sid: "S1", sub: false, tool: "read", debug: true, skill_nested: "ak-fix" },
    { ev: "tool", sid: "S1", sub: false, tool: "read" },
  ]);
});

test("skill rules get only the skill's name, typed or loaded", () => {
  for (const [prompt, skill] of [
    ["/skill:ak-cook build the tab", "ak-cook"], ["/ak:plan x", "ak:plan"], ["  /ak-advise", "ak-advise"],
    ['<skill name="ak-cook" location="x">\nbody\n</skill>\n\n--auto', "ak-cook"],
    ["please /ak-cook", ""], ["/Users/me/a.txt explain", ""], ["/", ""], ["", ""], [undefined, ""],
  ] as const) assert.equal(promptSkill(prompt), skill, String(prompt));
  assert.equal(skillRead("read", { path: "/u/.agents/skills/ak-journal/SKILL.md" }), "ak-journal");
  assert.equal(skillRead("read", { file_path: "C:\\s\\ak-git\\SKILL.md" }), "ak-git");
  assert.equal(skillRead("read", { path: "skills/ak-cook/references/x.md" }), "");
  assert.equal(skillRead("edit", { path: "skills/ak-cook/SKILL.md" }), "");

  const sent: any[] = [];
  const { pi, handlers } = fakePi();
  clawdHome(pi, (b) => sent.push(b), fakeStdin());
  const fire = (name: string, e: any = {}) => handlers.get(name)!(e, tui);
  fire("before_agent_start", { prompt: '<skill name="ak-cook" location="x">\nsecret body\n</skill>\n\ngo' });
  fire("tool_call", { toolName: "read", input: { path: "/s/ak-journal/SKILL.md" } });
  assert.deepEqual(sent, [
    { ev: "prompt", sid: "S1", sub: false, skill: "ak-cook" },
    { ev: "tool", sid: "S1", sub: false, tool: "read", skill_nested: "ak-journal" },
  ]);
  assert.ok(!JSON.stringify(sent).includes("secret"));   // the prompt never leaves pi
});

test("a question tool's own untitled dialog keeps the question phrase on the clock", () => {
  const sent: any[] = [];
  const { pi, handlers } = fakePi();
  clawdHome(pi, (b) => sent.push(b), fakeStdin());
  const fire = (name: string, e: any = {}, ctx: any = tui) => handlers.get(name)!(e, ctx);
  const other = { mode: "tui", sessionManager: { getSessionId: () => "S2" } };
  fire("tool_call", { toolName: "ask_user_question", input: { questions: [{ question: "Which anim?" }] } });
  fire("ui_prompt_start", {});                     // ask_user_question's ctx.ui.custom()
  fire("ui_prompt_start", {}, other);              // another session is not affected
  fire("ui_prompt_end");
  fire("tool_result", { toolName: "ask_user_question" });
  fire("ui_prompt_start", {});                     // a later plain dialog shows again
  assert.deepEqual(sent.filter((s) => s.ev === "attention").map((s) => [s.sid, s.msg]), [
    ["S1", undefined], ["S2", "pi needs you"], ["S1", "pi needs you"],
  ]);
});

test("no stdin listener for sub-sessions or a non-TTY stdin", () => {
  const { pi, handlers } = fakePi();
  const stdin = fakeStdin();
  clawdHome(pi, () => {}, stdin);
  handlers.get("ui_prompt_start")!({}, child);
  assert.equal(stdin.listenerCount("data"), 0);

  const b = fakePi();
  const piped = fakeStdin(false);
  clawdHome(b.pi, () => {}, piped);
  b.handlers.get("ui_prompt_start")!({}, tui);
  assert.equal(piped.listenerCount("data"), 0);
});

test("a throwing sender or session manager never escapes", () => {
  const { pi, handlers } = fakePi();
  clawdHome(pi, () => { throw new Error("boom"); }, fakeStdin());
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
