"""Agent adapters: safe config edits per agent, payload mapping, the event server route."""
import json
import os
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

import pytest

from clawd_desktop import agents
from clawd_desktop import events as ev
from clawd_desktop.agents.base import AgentError, merge_flat, merge_groups

from tests.test_hooks import LiveServer

SCRIPT = Path("/x/clawd-event.sh")


def reg(tmp_path, platform="darwin", which=None, env=None):
    return agents.registry(home=tmp_path, platform=platform, port=47913, script=SCRIPT,
                           which=which or (lambda _n: None), env=env or {})


# ---- generic merges -----------------------------------------------------------------
def test_merge_groups_and_flat_keep_foreign_entries():
    ours = lambda h: "clawd-event" in json.dumps(h)                       # noqa: E731
    data = {"x": 1, "hooks": {"Stop": [{"hooks": [{"command": "node mine.cjs"}]}]}}
    once = merge_groups(data, {"Stop": {"command": "clawd-event codex Stop"}}, ours)
    assert once["hooks"]["Stop"][0] == data["hooks"]["Stop"][0] and len(once["hooks"]["Stop"]) == 2
    assert merge_groups(once, None, ours) == data
    flat = {"version": 1, "hooks": {"stop": [{"command": "./audit.sh"}]}}
    f1 = merge_flat(flat, {"stop": {"command": "clawd-event cursor stop"}, "sessionEnd": {"command": "clawd-event"}}, ours)
    assert f1["hooks"]["stop"][0] == {"command": "./audit.sh"}
    assert merge_flat(f1, None, ours) == flat


# ---- each hook agent: install, idempotent, uninstall back to the original ---------------
@pytest.mark.parametrize("aid,rel,existing", [
    ("codex", ".codex/hooks.json",
     {"hooks": {"PostToolUse": [{"matcher": "Edit|Write", "hooks": [{"type": "command", "command": "node k.cjs"}]}]}}),
    ("gemini", ".gemini/settings.json",
     {"theme": "x", "hooks": {"BeforeTool": [{"matcher": "write_file", "hooks": [{"type": "command", "command": "sec.sh"}]}]}}),
    ("cursor", ".cursor/hooks.json", {"version": 1, "hooks": {"afterFileEdit": [{"command": "./format.sh"}]}}),
])
def test_hook_agents_install_and_remove_cleanly(tmp_path, aid, rel, existing):
    f = tmp_path / rel
    f.parent.mkdir(parents=True)
    f.write_text(json.dumps(existing))
    a = reg(tmp_path)[aid]
    a.prepare = lambda: None
    assert a.found() and not a.is_installed()
    a.install()
    data = json.loads(f.read_text())
    assert a.is_installed()
    assert list(f.parent.glob(f.name + ".bak-clawd-*"))                  # backed up first
    for k, v in existing.items():
        if k != "hooks":
            assert data[k] == v
    text = json.dumps(data)
    assert f"'{SCRIPT}' {aid} " in text
    before = f.read_text()
    a.install()
    assert f.read_text() == before                                        # idempotent
    a.uninstall()
    assert json.loads(f.read_text()) == existing and not a.is_installed()


def test_codex_hooks_run_in_the_background_with_a_windows_command(tmp_path):
    (tmp_path / ".codex").mkdir()
    a = reg(tmp_path)["codex"]
    a.prepare = lambda: None
    a.install()
    data = json.loads((tmp_path / ".codex/hooks.json").read_text())
    pre = data["hooks"]["PreToolUse"][0]["hooks"][0]
    assert pre["async"] is True and pre["timeout"] == 5
    assert pre["commandWindows"].startswith('cmd /c "curl.exe') and "/hook/codex?e=PreToolUse" in pre["commandWindows"]
    assert pre["commandWindows"].endswith('|| exit 0"')
    end = data["hooks"]["SessionEnd"][0]["hooks"][0]
    assert "async" not in end and end["timeout"] == 2
    assert "matcher" not in data["hooks"]["PreToolUse"][0]


def test_windows_commands_are_cmd_wrapped_curl(tmp_path):
    (tmp_path / ".cursor").mkdir()
    a = reg(tmp_path, platform="win32")["cursor"]
    a.install()                                                           # win32: no script to write
    cmd = json.loads((tmp_path / ".cursor/hooks.json").read_text())["hooks"]["stop"][0]["command"]
    assert cmd == ('cmd /c "curl.exe -s -o NUL --max-time 0.3 --connect-timeout 0.2 '
                   '--data-binary @- http://127.0.0.1:47913/hook/cursor?e=stop || exit 0"')


def test_cursor_uses_only_observing_hooks(tmp_path):
    (tmp_path / ".cursor").mkdir()
    a = reg(tmp_path)["cursor"]
    a.prepare = lambda: None
    a.install()
    names = set(json.loads((tmp_path / ".cursor/hooks.json").read_text())["hooks"])
    blocking = {"preToolUse", "beforeShellExecution", "beforeMCPExecution", "beforeReadFile",
                "beforeTabFileRead", "subagentStart"}
    assert not names & blocking


def test_broken_config_is_never_touched(tmp_path):
    f = tmp_path / ".gemini/settings.json"
    f.parent.mkdir()
    f.write_text("{ // comments")
    a = reg(tmp_path)["gemini"]
    a.prepare = lambda: None
    with pytest.raises(AgentError):
        a.install()
    assert f.read_text() == "{ // comments"
    assert a.info()["error"]


def test_an_agent_does_not_claim_another_agents_hooks(tmp_path):
    r = reg(tmp_path)
    codex_hook = {"command": r["codex"].posix_command("Stop"), "commandWindows": r["codex"].windows_command("Stop")}
    assert r["codex"].ours(codex_hook)
    assert not r["gemini"].ours(codex_hook) and not r["cursor"].ours(codex_hook)
    assert not r["codex"].ours({"command": "node /x/codex-helper.cjs"})


def test_gemini_groups_have_no_matcher(tmp_path):
    (tmp_path / ".gemini").mkdir()
    (tmp_path / ".gemini/settings.json").write_text("{}")
    a = reg(tmp_path)["gemini"]
    a.prepare = lambda: None
    a.install()
    data = json.loads((tmp_path / ".gemini/settings.json").read_text())
    assert all("matcher" not in g for groups in data["hooks"].values() for g in groups)   # "*" is not a valid regex
    assert data["hooks"]["BeforeTool"][0]["hooks"][0]["timeout"] == 2000                  # milliseconds


def test_gemini_needs_more_than_the_antigravity_folder(tmp_path):
    (tmp_path / ".gemini/antigravity-ide").mkdir(parents=True)
    assert not reg(tmp_path)["gemini"].found()
    assert reg(tmp_path, which=lambda n: "/usr/local/bin/gemini" if n == "gemini" else None)["gemini"].found()


def test_copilot_writes_its_own_file_and_removes_it(tmp_path):
    (tmp_path / ".copilot").mkdir()
    a = reg(tmp_path)["copilot"]
    a.prepare = lambda: None
    a.install()
    f = tmp_path / ".copilot/hooks/clawd.json"
    data = json.loads(f.read_text())
    assert data["version"] == 1 and "PreToolUse" in data["hooks"] and "notification" in data["hooks"]
    h = data["hooks"]["PreToolUse"][0]
    assert h["bash"] == f"'{SCRIPT}' copilot PreToolUse" and h["powershell"].startswith("cmd /c")
    a.uninstall()
    assert not f.exists() and not list(f.parent.glob("*.json*"))


def test_copilot_never_overwrites_a_foreign_file(tmp_path):
    f = tmp_path / ".copilot/hooks/clawd.json"
    f.parent.mkdir(parents=True)
    f.write_text('{"version": 1, "hooks": {}}')
    a = reg(tmp_path)["copilot"]
    a.prepare = lambda: None
    with pytest.raises(AgentError):
        a.install()


def test_env_dirs_move_the_config(tmp_path):
    r = reg(tmp_path, env={"CODEX_HOME": str(tmp_path / "cx"), "COPILOT_HOME": str(tmp_path / "cp"),
                           "PI_CODING_AGENT_DIR": str(tmp_path / "pp"),
                           "XDG_CONFIG_HOME": str(tmp_path / "xdg")})
    assert r["codex"].file() == tmp_path / "cx/hooks.json"
    assert r["copilot"].file() == tmp_path / "cp/hooks/clawd.json"
    assert r["pi"].target() == tmp_path / "pp/extensions/clawd-home.ts"
    assert r["opencode"].target() == tmp_path / "xdg/opencode/plugins/clawd.js"


# ---- plugin agents (pi, opencode) ---------------------------------------------------
@pytest.mark.parametrize("aid", ["pi", "opencode"])
def test_plugin_agents_copy_update_and_remove(tmp_path, aid):
    a = reg(tmp_path)[aid]
    a.plugin_dir().mkdir(parents=True)
    assert not a.is_installed()
    a.install()
    t = a.target()
    assert t.read_text() == a.source().read_text() and a.is_installed()
    t.write_text(t.read_text() + "\n// old copy\n")
    assert a.needs_update() and a.refresh() and t.read_text() == a.source().read_text()
    assert not a.refresh()
    a.uninstall()
    assert not t.exists()


def test_plugin_agent_leaves_a_foreign_file_and_keeps_a_dev_symlink(tmp_path):
    a = reg(tmp_path)["pi"]
    t = a.target()
    t.parent.mkdir(parents=True)
    t.write_text("// someone else's extension")
    with pytest.raises(AgentError):
        a.install()
    assert t.read_text() == "// someone else's extension"
    t.unlink()
    os.symlink(a.source(), t)
    assert a.is_installed() and a.install().startswith("Đã nối") and t.is_symlink()
    assert not a.needs_update()
    a.uninstall()
    assert not t.exists() and a.source().exists()


def test_uninstall_all_reports_every_agent(tmp_path):
    out = agents.uninstall_all({k: v for k, v in reg(tmp_path).items() if k != "claude"})
    assert [line.split(":")[0] for line in out] == ["pi", "codex", "gemini", "cursor", "opencode", "copilot"]


# ---- payloads -> tracker events ---------------------------------------------------------
def kinds(evs):
    return [e["ev"] for e in evs]


def test_codex_payloads():
    a = agents.registry(home=Path("/nonexistent"))["codex"]
    base = {"session_id": "thr", "cwd": "/w"}
    assert kinds(a.normalize({**base, "hook_event_name": "UserPromptSubmit", "prompt": "fix the bug"})) == ["prompt"]
    assert a.normalize({**base, "hook_event_name": "UserPromptSubmit", "prompt": "fix it"})[0]["debug"] is True
    tool = a.normalize({**base, "hook_event_name": "PreToolUse", "tool_name": "Bash",
                        "tool_input": {"command": "pytest -q"}})[0]
    assert tool["ev"] == "tool" and tool["cmd"] == "pytest -q" and tool["sid"] == "codex:thr"
    perm = a.normalize({**base, "hook_event_name": "PermissionRequest", "tool_name": "Bash",
                        "tool_input": {"command": "rm -rf build"}})[0]
    assert perm["ev"] == "attention" and perm["msg"] == "Allow Bash: rm -rf build"
    assert kinds(a.normalize({**base, "hook_event_name": "Interrupt"})) == ["abort"]
    assert kinds(a.normalize({**base}, "Stop")) == ["stop"]                    # name from ?e=
    assert a.normalize({"hook_event_name": "Stop"}) == [] and a.normalize("x") == []


def test_gemini_payloads():
    a = agents.registry(home=Path("/nonexistent"))["gemini"]
    base = {"session_id": "g1"}
    assert kinds(a.normalize({**base, "hook_event_name": "BeforeAgent", "prompt": "hi"})) == ["prompt"]
    assert kinds(a.normalize({**base, "hook_event_name": "BeforeTool", "tool_name": "run_shell_command"})) == ["tool"]
    assert kinds(a.normalize({**base, "hook_event_name": "AfterTool", "tool_response": {"error": "boom"}})) == ["tool_error"]
    assert kinds(a.normalize({**base, "hook_event_name": "AfterTool", "tool_response": {"llmContent": "ok"}})) == ["tool_done"]
    n = a.normalize({**base, "hook_event_name": "Notification", "notification_type": "ToolPermission",
                     "message": "Allow write_file?"})
    assert n[0]["ev"] == "attention" and n[0]["msg"] == "Allow write_file?"
    assert kinds(a.normalize({**base, "hook_event_name": "AfterAgent"})) == ["stop"]


def test_cursor_payloads():
    a = agents.registry(home=Path("/nonexistent"))["cursor"]
    base = {"conversation_id": "c1", "cursor_version": "1.7.2"}
    assert kinds(a.normalize({**base, "hook_event_name": "beforeSubmitPrompt", "prompt": "x"})) == ["prompt"]
    done = a.normalize({**base, "hook_event_name": "postToolUse", "tool_name": "MCP:read_file"})
    assert kinds(done) == ["tool", "tool_done"] and done[0]["tool"] == "read_file"
    assert kinds(a.normalize({**base, "hook_event_name": "postToolUseFailure", "is_interrupt": True})) == ["abort"]
    assert kinds(a.normalize({**base, "hook_event_name": "stop", "status": "error"})) == ["stop_error"]
    assert kinds(a.normalize({**base, "hook_event_name": "stop", "status": "aborted"})) == ["abort"]


def test_copilot_payloads_in_both_casings():
    a = agents.registry(home=Path("/nonexistent"))["copilot"]
    snake = {"hook_event_name": "PreToolUse", "session_id": "p1", "tool_name": "Bash", "tool_input": {"command": "ls"}}
    assert kinds(a.normalize(snake)) == ["tool"]
    assert kinds(a.normalize({**snake, "tool_name": "AskUserQuestion"})) == ["attention"]
    camel = {"sessionId": "p1", "notification_type": "permission_prompt", "message": "Allow bash?"}
    assert a.normalize(camel, "notification")[0]["msg"] == "Allow bash?"
    assert a.normalize({"sessionId": "p1", "notification_type": "agent_idle"}, "notification") == []
    assert kinds(a.normalize({"sessionId": "p1", "agentName": "x"}, "subagentStart")) == ["subagent_start"]
    assert a.normalize({"session_id": "p1", "hook_event_name": "ErrorOccurred", "recoverable": True}) == []


def test_claude_hook_drops_cursor_payloads():
    p = {"hook_event_name": "PreToolUse", "session_id": "s", "tool_name": "Shell", "cursor_version": "1.7"}
    assert ev.claude_event(p) is None
    assert ev.claude_event({k: v for k, v in p.items() if k != "cursor_version"})["ev"] == "tool"


def test_new_tool_names_read_as_actions():
    for name, action in [("apply_patch", "edit"), ("run_shell_command", "run"), ("Shell", "run"),
                         ("read_file", "read"), ("view", "read"), ("str_replace_editor", "edit"),
                         ("google_web_search", "browse"), ("update_plan", "plan"), ("write_todos", "plan")]:
        assert ev.tool_action(name) == action, name
    assert ev.is_question_tool("request_user_input")


# ---- the event server route ----------------------------------------------------------------
def post(port, path, body):
    req = urllib.request.Request(f"http://127.0.0.1:{port}{path}", data=json.dumps(body).encode(),
                                 method="POST")
    with urllib.request.urlopen(req, timeout=5) as r:
        return r.status


def test_event_server_routes_every_agent():
    with LiveServer() as s:
        port = s.server.port
        assert post(port, "/hook/codex?e=UserPromptSubmit", {"session_id": "t1"}) == 200
        assert post(port, "/hook/gemini", {"hook_event_name": "BeforeTool", "session_id": "g",
                                           "tool_name": "read_file"}) == 200
        assert post(port, "/hook/opencode", {"ev": "prompt", "sid": "o"}) == 200
        assert post(port, "/hook/pi", {"ev": "prompt", "sid": "p"}) == 200
        assert post(port, "/hook/claude", {"hook_event_name": "UserPromptSubmit", "session_id": "c"}) == 200
        with pytest.raises(urllib.error.HTTPError) as e:
            post(port, "/hook/nobody", {})
        assert e.value.code == 404
        time.sleep(0.05)
        sids = {x["sid"] for x in s.brain.tracker.snapshot(time.time())}
        assert {"codex:t1", "gemini:g", "opencode:o", "pi:p", "claude:c"} <= sids


@pytest.mark.skipif(sys.platform == "win32", reason="sh hook is the macOS/Linux path")
def test_the_hook_script_passes_agent_and_event(tmp_path):
    from clawd_desktop import hooks
    script = hooks.write_script(tmp_path / "clawd-event.sh", port=47913)
    with LiveServer() as s:
        payload = json.dumps({"session_id": "via-sh"}).encode()
        p = subprocess.run(["sh", str(script), "codex", "UserPromptSubmit"], input=payload,
                           env={"PATH": "/usr/bin:/bin", "CLAWD_EVENT_PORT": str(s.server.port)},
                           capture_output=True, timeout=10)
        assert p.returncode == 0 and p.stdout == b""
        time.sleep(0.05)
        assert any(x["sid"] == "codex:via-sh" for x in s.brain.tracker.snapshot(time.time()))
