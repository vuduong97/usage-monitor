// clawd.js — opencode plugin: report agent lifecycle to the Clawd desktop app
// on 127.0.0.1, which turns it into what the desk clock shows.
//
// Copied into ~/.config/opencode/plugins/ by the app (Cài đặt -> Agent). State-only
// and fire-and-forget: every hook returns at once, never changes args or output,
// each POST gives up after 300 ms and any error is swallowed. No app running = no
// effect. Events follow https://opencode.ai/docs/plugins ; fields are read
// defensively because the plugin API is still moving.
import http from "node:http";

const PORT = Number(process.env.CLAWD_EVENT_PORT) || 47913;
const QUESTION_TOOLS = new Set(["question", "ask_user", "ask_user_question"]);

function post(body, port = PORT) {
  try {
    const data = JSON.stringify(body);
    const req = http.request(
      { host: "127.0.0.1", port, path: "/hook/opencode", method: "POST", timeout: 300,
        headers: { "Content-Type": "application/json", "Content-Length": Buffer.byteLength(data) } },
      (res) => res.resume(),
    );
    req.on("error", () => {});
    req.on("timeout", () => req.destroy());
    req.end(data);
  } catch {
    /* never let reporting affect opencode */
  }
}

const sidOf = (p) => (p && (p.sessionID || p.sessionId || (p.info && (p.info.sessionID || p.info.id)))) || "";

function makeHooks(send = post) {
  const children = new Set();     // sessions a task tool started: subagents
  const prompted = new Set();     // user message ids already reported
  const busy = new Set();
  const report = (ev, sid, extra = {}) => {
    if (!sid) return;
    send({ ev, sid, sub: children.has(sid), ...extra });
  };
  return {
    event: async ({ event } = {}) => {
      try {
        const t = event && event.type;
        const p = (event && event.properties) || {};
        if (t === "session.created") {
          const info = p.info || {};
          if (info.parentID && info.id) children.add(info.id);
          report("start", info.id);
        } else if (t === "message.updated") {
          const info = p.info || {};
          if (info.role === "user" && info.id && !prompted.has(info.id)) {
            prompted.add(info.id);
            if (prompted.size > 500) prompted.clear();
            busy.add(info.sessionID);
            report("prompt", info.sessionID);
          }
        } else if (t === "session.status") {
          const sid = sidOf(p), kind = p.status && p.status.type;
          if (kind === "busy" && !busy.has(sid)) { busy.add(sid); report("prompt", sid); }
          if (kind === "idle" && busy.delete(sid)) report("stop", sid);
        } else if (t === "session.idle") {
          const sid = sidOf(p);
          if (busy.delete(sid)) report("stop", sid);
        } else if (t === "session.error") {
          const sid = sidOf(p);
          busy.delete(sid);
          const aborted = p.error && /abort/i.test(String(p.error.name || ""));
          report(aborted ? "abort" : "stop_error", sid);
        } else if (t === "session.compacted") {
          report("compact_done", sidOf(p));
        } else if (t === "permission.asked" || t === "permission.updated") {
          const what = p.action || p.permission || p.type || p.title || "";
          report("attention", sidOf(p), { msg: what ? `Allow ${what}` : "opencode needs you" });
        } else if (t === "permission.replied") {
          report("attention_done", sidOf(p));
        } else if (t === "session.deleted") {
          const sid = sidOf(p);
          busy.delete(sid);
          children.delete(sid);
          report("end", sid);
        }
      } catch {
        /* ignore */
      }
    },
    "tool.execute.before": async (input) => {
      try {
        const tool = String((input && input.tool) || "");
        const sid = sidOf(input);
        if (QUESTION_TOOLS.has(tool.toLowerCase())) return report("attention", sid, { tool });
        if (tool === "task") report("subagent_start", sid);
        report("tool", sid, { tool });
      } catch {
        /* ignore */
      }
    },
    "tool.execute.after": async (input) => {
      try {
        const tool = String((input && input.tool) || "");
        const sid = sidOf(input);
        if (QUESTION_TOOLS.has(tool.toLowerCase())) report("attention_done", sid);
        if (tool === "task") report("subagent_stop", sid);
        report("tool_done", sid, { tool });
      } catch {
        /* ignore */
      }
    },
  };
}

// opencode calls every exported function as a plugin: export only this one. The
// helpers ride on it for the tests (integrations/opencode/clawd.test.mjs).
export const ClawdPlugin = async () => makeHooks();
ClawdPlugin.makeHooks = makeHooks;
ClawdPlugin.post = post;
