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
// Install: integrations/pi/install.sh (symlinks this file into
// ~/.pi/agent/extensions/).
import http from "node:http";

type Ctx = { mode?: string; sessionManager?: { getSessionId?: () => string } };
type Pi = { on: (event: string, handler: (e: any, ctx: Ctx) => unknown) => unknown };

const PORT = Number(process.env.CLAWD_EVENT_PORT) || 47913;
const SUBAGENT_TOOLS = new Set(["subagent"]);     // pi-subagents
const QUESTION_TOOLS = new Set(["ask_user_question", "ask_user", "question"]);

export function sessionId(ctx: Ctx): string {
  try {
    return String(ctx?.sessionManager?.getSessionId?.() || "default");
  } catch {
    return "default";
  }
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

export default function clawdHome(pi: Pi, send: (b: Record<string, unknown>) => void = post) {
  const emit = (ev: string, ctx: Ctx, extra: Record<string, unknown> = {}) => {
    try {
      send({ ev, sid: sessionId(ctx), sub: ctx?.mode !== undefined && ctx.mode !== "tui", ...extra });
    } catch {
      /* ignore */
    }
    return undefined;
  };

  pi.on("session_start", (_e, ctx) => emit("start", ctx));
  pi.on("before_agent_start", (_e, ctx) => emit("prompt", ctx));
  pi.on("tool_call", (e, ctx) => {
    const tool = String(e?.toolName ?? "");
    if (QUESTION_TOOLS.has(tool)) return emit("attention", ctx, { tool, msg: firstQuestion(e?.input) || "pi has a question" });
    emit("tool", ctx, { tool });
    if (SUBAGENT_TOOLS.has(tool)) emit("subagent_start", ctx, { tool });
    return undefined;
  });
  pi.on("tool_result", (e, ctx) => {
    const tool = String(e?.toolName ?? "");
    if (SUBAGENT_TOOLS.has(tool)) emit("subagent_stop", ctx, { tool });
    return emit(e?.isError ? "tool_error" : "tool_done", ctx, { tool });
  });
  pi.on("ui_prompt_start", (e, ctx) => emit("attention", ctx, { msg: String(e?.title ?? "") || "pi needs you" }));
  pi.on("ui_prompt_end", (_e, ctx) => emit("attention_done", ctx));
  pi.on("session_before_compact", (_e, ctx) => emit("compact", ctx));
  pi.on("session_compact", (_e, ctx) => emit("compact_done", ctx));
  pi.on("session_compact_failed", (_e, ctx) => emit("compact_done", ctx));
  pi.on("agent_settled", (e, ctx) => emit(e?.aborted ? "abort" : "stop", ctx));
  pi.on("session_shutdown", (_e, ctx) => emit("end", ctx));
}
