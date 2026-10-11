// clawd-home.ts — pi extension: report agent lifecycle to the clawd-home daemon
// on 127.0.0.1, which turns it into what the desk clock shows.
//
// State-only and fire-and-forget: every handler returns undefined at once (it
// never blocks, changes or cancels anything pi does), each POST gives up after
// 300 ms, and any error is swallowed. No daemon running = no effect.
//
// A pi that is not the interactive TUI (print/json/rpc mode, e.g. the child
// processes pi-subagents starts) reports as a sub-session: the clock counts
// it as a subagent and never celebrates its turns.
//
// "Needs you" clears once the user goes to that session: while it is pending,
// the first focus-in, mouse click or keypress on the terminal sends one
// `attention_seen`. That input is read by a passive extra listener on
// process.stdin (pi itself reads stdin the same way); ctx.ui.onTerminalInput
// is not enough because in fullscreen mode pi's viewport consumes focus-in
// and clicks before extension listeners see them.
//
// Install: integrations/pi/install.sh (symlinks this file into
// ~/.pi/agent/extensions/).
import http from "node:http";

type Ctx = { mode?: string; sessionManager?: { getSessionId?: () => string } };
type Pi = { on: (event: string, handler: (e: any, ctx: Ctx) => unknown) => unknown };
type Input = {
  isTTY?: boolean;
  on: (event: "data", listener: (chunk: unknown) => void) => unknown;
  off: (event: "data", listener: (chunk: unknown) => void) => unknown;
};

const PORT = Number(process.env.CLAWD_EVENT_PORT) || 47913;
const SUBAGENT_TOOLS = new Set(["subagent"]);     // pi-subagents
const QUESTION_TOOLS = new Set(["ask_user_question", "ask_user", "question"]);

// Debug turns (the daemon's is_debug_prompt / is_debug_skill_load, kept in step):
// only the verdict is sent, never the prompt. Skill bodies pi expanded into the
// prompt count by their name only; "loi" alone is not enough ("l\u1eddi" = reply).
const DEBUG_SKILLS = /^(?:ak[:-])?(?:debug|fix)$/i;
const DEBUG_WORDS = /(?<![\p{L}\p{N}_])(?:\/(?:ak:|skill:ak-)(?:debug|fix)|debug[\p{L}\p{N}_]*|fix|bugs?|l\u1ed7i)(?![\p{L}\p{N}_])/iu;
const SKILL_BLOCK = /<skill\b[^>]*>[\s\S]*?(?:<\/skill>|$)/g;
const SKILL_NAME = /<skill\s+name="([^"]+)"/g;
const DEBUG_SKILL_FILE = /(?:^|[/\\])ak-(?:debug|fix)[/\\]SKILL\.md$/i;

export function isDebugPrompt(text: unknown): boolean {
  if (typeof text !== "string") return false;
  const t = text.normalize("NFC");
  for (const m of t.matchAll(SKILL_NAME)) if (DEBUG_SKILLS.test(m[1])) return true;
  return DEBUG_WORDS.test(t.replace(SKILL_BLOCK, " "));
}

export function isDebugSkillRead(tool: string, input: any): boolean {
  const path = input?.path ?? input?.file_path;
  return tool === "read" && typeof path === "string" && DEBUG_SKILL_FILE.test(path);
}

export function sessionId(ctx: Ctx): string {
  try {
    return String(ctx?.sessionManager?.getSessionId?.() || "default");
  } catch {
    return "default";
  }
}

// Escape sequences in a stdin chunk, longest forms first: DCS/SOS/PM/APC and
// OSC strings (terminal replies), legacy X10 mouse, CSI, SS3.
// Built from ESC/BEL strings: matching control characters is the point here.
const ESC = String.fromCharCode(27);
const BEL = String.fromCharCode(7);
const ST = `(?:${BEL}|${ESC}\\\\)`;
const ESCAPES = new RegExp(
  [
    `${ESC}[P_^X][\\s\\S]*?${ST}`,          // DCS/SOS/PM/APC
    `${ESC}\\][\\s\\S]*?${ST}`,             // OSC
    `${ESC}\\[M[\\s\\S]{3}`,                 // X10 mouse
    `${ESC}\\[([0-?]*)[ -/]*([@-~])`,         // CSI
    `${ESC}O[@-~]`,                           // SS3
  ].join("|"),
  "g",
);
const KEY_FINALS = new Set("ABCDEFHPQSZ~u");     // cursor/F-keys, Shift-Tab, ~-keys, kitty CSI-u

/** Whether a raw stdin chunk holds something the user did: focus-in, a mouse
 *  press, or a key. Terminal replies (colors, sizes, device attributes, mode
 *  reports), focus-out, mouse motion, wheel and releases do not count. */
export function isUserInput(chunk: unknown): boolean {
  let s: string;
  try {
    if (typeof chunk === "string") s = chunk;
    else if (Buffer.isBuffer(chunk)) s = chunk.toString("utf8");
    else return false;
  } catch {
    return false;
  }
  let found = false;
  const rest = s.replace(ESCAPES, (seq: string, csiParams?: string, final?: string) => {
    const params = csiParams ?? "";
    if (found) return "";
    if (seq.startsWith(`${ESC}O`)) found = true;                         // SS3 key
    else if (final === undefined) return "";                            // OSC/DCS/APC/X10
    else if (params === "" && final === "I") found = true;              // focus in
    else if (params.startsWith("<")) {                                  // SGR mouse
      const b = Number(params.slice(1).split(";")[0]);
      found = final === "M" && (b & 0xe0) === 0 && (b & 3) !== 3;       // press, not motion/wheel
    } else if (!/^[?<>=]/.test(params)) found = KEY_FINALS.has(final);  // private = a reply
    return "";
  });
  return found || rest.length > 0;                                       // typed text, Enter, Esc
}

export function post(body: Record<string, unknown>, port = PORT): void {
  try {
    const data = JSON.stringify(body);
    const req = http.request(
      { host: "127.0.0.1", port, path: "/hook/pi", method: "POST", timeout: 300,
        headers: { "Content-Type": "application/json", "Content-Length": Buffer.byteLength(data) } },
      (res) => res.resume(),
    );
    req.on("error", () => {});
    req.on("timeout", () => req.destroy());
    req.end(data);
  } catch {
    /* never let reporting affect pi */
  }
}

function firstQuestion(input: any): string {
  try {
    const q = input?.questions?.[0]?.question ?? input?.question;
    return typeof q === "string" ? q : "";
  } catch {
    return "";
  }
}

const CLEARS_ATTENTION = new Set(["attention_done", "prompt", "stop", "abort", "end"]);

export default function clawdHome(
  pi: Pi,
  send: (b: Record<string, unknown>) => void = post,
  input: Input | undefined = process.stdin,
) {
  let pending: string | undefined;          // sid whose "needs you" the user has not seen yet
  let listening = false;
  let asking: string | undefined;           // sid with a question tool (ask_user_question...) open
  const onInput = (chunk: unknown) => {
    try {
      if (pending === undefined || !isUserInput(chunk)) return;
      const sid = pending;
      pending = undefined;                  // one `attention_seen` per question
      send({ ev: "attention_seen", sid, sub: false });
    } catch {
      /* never let reporting affect pi */
    }
  };
  const listen = (on: boolean) => {
    try {
      if (on === listening || !input?.isTTY) return;
      if (on) input.on("data", onInput);
      else input.off("data", onInput);
      listening = on;
    } catch {
      /* ignore */
    }
  };

  const emit = (ev: string, ctx: Ctx, extra: Record<string, unknown> = {}) => {
    try {
      const sid = sessionId(ctx);
      const sub = ctx?.mode !== undefined && ctx.mode !== "tui";
      if (ev === "attention" && !sub) {
        pending = sid;
        listen(true);
      } else if (pending === sid && (CLEARS_ATTENTION.has(ev) || QUESTION_TOOLS.has(String(extra.tool ?? "")))) {
        pending = undefined;                // answered or moved on before the user came
      }
      if (ev === "end") listen(false);
      send({ ev, sid, sub, ...extra });
    } catch {
      /* ignore */
    }
    return undefined;
  };

  pi.on("session_start", (_e, ctx) => emit("start", ctx));
  pi.on("before_agent_start", (e, ctx) => emit("prompt", ctx, isDebugPrompt(e?.prompt) ? { debug: true } : {}));
  pi.on("tool_call", (e, ctx) => {
    const tool = String(e?.toolName ?? "");
    if (QUESTION_TOOLS.has(tool)) {
      asking = sessionId(ctx);
      return emit("attention", ctx, { tool, msg: firstQuestion(e?.input) || "pi has a question" });
    }
    const cmd = e?.input?.command;      // lets the daemon spot test runs ("debugger")
    const extra: Record<string, unknown> = typeof cmd === "string" ? { tool, cmd: cmd.slice(0, 300) } : { tool };
    if (isDebugSkillRead(tool, e?.input)) extra.debug = true;
    emit("tool", ctx, extra);
    if (SUBAGENT_TOOLS.has(tool)) emit("subagent_start", ctx, { tool });
    return undefined;
  });
  pi.on("tool_result", (e, ctx) => {
    const tool = String(e?.toolName ?? "");
    if (SUBAGENT_TOOLS.has(tool)) emit("subagent_stop", ctx, { tool });
    if (QUESTION_TOOLS.has(tool) && asking === sessionId(ctx)) asking = undefined;
    return emit(e?.isError ? "tool_error" : "tool_done", ctx, { tool });
  });
  pi.on("ui_prompt_start", (e, ctx) => {
    // A question tool opens its own (untitled) dialog: keep showing its question
    // instead of replacing it with the generic text.
    if (asking !== undefined && asking === sessionId(ctx)) return undefined;
    return emit("attention", ctx, { msg: String(e?.title ?? "") || "pi needs you" });
  });
  pi.on("ui_prompt_end", (_e, ctx) => emit("attention_done", ctx));
  pi.on("session_before_compact", (_e, ctx) => emit("compact", ctx));
  pi.on("session_compact", (_e, ctx) => emit("compact_done", ctx));
  pi.on("session_compact_failed", (_e, ctx) => emit("compact_done", ctx));
  pi.on("agent_settled", (e, ctx) => {
    asking = undefined;
    return emit(e?.aborted ? "abort" : "stop", ctx);
  });
  pi.on("session_shutdown", (_e, ctx) => emit("end", ctx));
}
