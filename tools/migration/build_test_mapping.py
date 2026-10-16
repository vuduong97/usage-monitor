#!/usr/bin/env python3
"""Build plans/261015-0726-rust-tauri-port/reports/test-mapping.md (phase 14).

Every Python test function of `tests/` gets one row: ported / partial / dropped / python-only /
gap. A function is "ported by name" when a Rust test has the same name (minus `test_`); every
other function comes from `test_mapping_data.MANUAL`. Every Rust reference is checked against the
real test names, so the report cannot name a test that does not exist, and a Python test that is
in neither place makes the script fail (no empty rows).

Read-only with respect to the repos; writes only the report. Usage (from the worktree root):

    python tools/migration/build_test_mapping.py \
        --python-repo D:/vu/usage-monitor \
        --python D:/vu/usage-monitor/.venv/Scripts/python.exe
"""
from __future__ import annotations

import argparse
import collections
import json
import os
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from test_mapping_data import MANUAL  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
RUST = ROOT / "desktop-rs"
REPORT = ROOT / "plans/261015-0726-rust-tauri-port/reports/test-mapping.md"

ALIASES = {
    "agents/": "crates/clawd-agents/", "appupdate/": "crates/clawd-appupdate/",
    "clock/": "crates/clawd-clock/", "config/": "crates/clawd-config/",
    "events/": "crates/clawd-events/", "firmware/": "crates/clawd-firmware/",
    "store/": "crates/clawd-store/", "usage/": "crates/clawd-usage/",
    "app/": "app/", "e2e/": "tests-e2e/",
}
TEST_RE = re.compile(
    r"#\[(?:tokio::)?test[^\]]*\]\s*(?:#\[[^\]]*\]\s*)*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)"
)


def scan_rust() -> dict[str, list[str]]:
    found: dict[str, list[str]] = {}
    for dirpath, dirnames, files in os.walk(RUST):
        dirnames[:] = [d for d in dirnames if d not in ("target", "spike", "node_modules")]
        for name in files:
            if name.endswith(".rs"):
                path = Path(dirpath) / name
                names = TEST_RE.findall(path.read_text(encoding="utf8", errors="replace"))
                if names:
                    found[path.relative_to(RUST).as_posix()] = names
    return found


def collect_python(python: str, repo: str) -> collections.OrderedDict[str, int]:
    out = subprocess.run(
        [python, "-m", "pytest", "--collect-only", "-q", "-o", "addopts=", "tests"],
        cwd=repo, capture_output=True, text=True, check=True,
    ).stdout
    funcs: collections.OrderedDict[str, int] = collections.OrderedDict()
    for line in out.splitlines():
        if "::" in line:
            key = re.sub(r"\[.*\]$", "", line.strip())
            funcs[key] = funcs.get(key, 0) + 1
    return funcs


def expand(ref: str) -> tuple[str, str]:
    """`events/tests/golden.rs::fn[case]` -> (path under desktop-rs, fn)."""
    path, _, fn = ref.partition("::")
    fn = re.sub(r"\[.*\]$", "", fn)
    for short, full in ALIASES.items():
        if path.startswith(short):
            return full + path[len(short):], fn
    raise SystemExit(f"unknown reference prefix: {ref}")


def scenario_names() -> set[str]:
    data = json.loads((RUST / "fixtures/events/scenarios.json").read_text(encoding="utf8"))
    return set(data["output"])


def check_ref(ref: str, rust: dict[str, list[str]], scenarios: set[str]) -> None:
    path, fn = expand(ref)
    if fn not in rust.get(path, []):
        raise SystemExit(f"reference to a Rust test that does not exist: {ref}")
    case = re.search(r"\[(.*)\]$", ref)
    if case and ref.startswith("events/tests/golden.rs::every_brain"):
        name = case.group(1)
        ok = any(s.startswith(name[:-1]) for s in scenarios) if name.endswith("*") else name in scenarios
        if not ok:
            raise SystemExit(f"unknown golden scenario in {ref}")


def short(ref: str) -> str:
    path, _, fn = ref.partition("::")
    return f"`{Path(path).name}::{fn}`"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--python-repo", default="D:/vu/usage-monitor")
    ap.add_argument("--python", default="D:/vu/usage-monitor/.venv/Scripts/python.exe")
    ap.add_argument("--out", default=str(REPORT))
    args = ap.parse_args()

    funcs = collect_python(args.python, args.python_repo)
    rust = scan_rust()
    scenarios = scenario_names()
    by_name: dict[str, list[str]] = collections.defaultdict(list)
    for path, names in rust.items():
        for n in names:
            by_name[n].append(path)

    rows = []
    unknown = []
    for key, cases in funcs.items():
        file, name = key.split("::")
        short_name = name[len("test_"):] if name.startswith("test_") else name
        manual = MANUAL.get(name)
        if manual:
            status, refs, note = manual
        else:
            hits = by_name.get(short_name) or by_name.get(name)
            if not hits:
                unknown.append(key)
                continue
            status, note = "ported", "same name"
            refs = [f"{h}::{short_name if short_name in rust[h] else name}" for h in hits[:3]]
            # by-name hits are paths under desktop-rs: turn them into alias refs for the checker
            refs = [_alias_of(r) for r in refs]
        for ref in refs:
            check_ref(ref, rust, scenarios)
        rows.append((Path(file).stem, name, cases, status, refs, note))
    stale = sorted(set(MANUAL) - {k.split("::")[1] for k in funcs})
    if unknown or stale:
        print("no mapping for:", *unknown, sep="\n  ")
        print("MANUAL entries for tests that no longer exist:", *stale, sep="\n  ")
        return 1

    Path(args.out).write_text(render(rows, rust), encoding="utf8", newline="\n")
    print(f"wrote {args.out}: {len(rows)} functions")
    return 0


def _alias_of(rel: str) -> str:
    for short_, full in ALIASES.items():
        if rel.startswith(full):
            return short_ + rel[len(full):]
    raise SystemExit(f"cannot alias {rel}")


def render(rows, rust) -> str:
    total_funcs = len(rows)
    total_cases = sum(r[2] for r in rows)
    status_funcs = collections.Counter(r[3] for r in rows)
    status_cases = collections.Counter()
    for r in rows:
        status_cases[r[3]] += r[2]
    rust_total = sum(len(v) for p, v in rust.items() if not p.startswith("tests-e2e/"))
    e2e_total = sum(len(v) for p, v in rust.items() if p.startswith("tests-e2e/"))
    order = ["ported", "partial", "dropped", "python-only", "gap"]
    lines = [
        "# Python test -> Rust mapping (phase 14)",
        "",
        "Generated by `tools/migration/build_test_mapping.py` from `pytest --collect-only` on the Python repo",
        "and a scan of `desktop-rs/**/*.rs`; the hand-written part is `tools/migration/test_mapping_data.py`",
        "(built from the phase worker reports, re-checked against the Rust tree). Every Rust name below was",
        "verified to exist when this file was generated. Re-run the script after any test rename.",
        "",
        "## Count: the plan's 413 vs the real number",
        "",
        f"* **413 is the number of test *functions*.** `pytest --collect-only` collects **{total_cases}** items:",
        f"  {total_cases - total_funcs} more because of `@pytest.mark.parametrize` ({total_funcs} functions, {total_cases} cases).",
        "  Both are listed below; the case count is in the `cases` column. The plan's number is correct as a function count,",
        "  but any 'tests ported' claim by cases has to use "
        f"{total_cases}.",
        f"* Rust side: {rust_total} `#[test]`/`#[tokio::test]` functions in the workspace crates + app (plus {e2e_total} in",
        "  `tests-e2e`, phase 14). `packaging-rs/test_check_version.py` (8 Python tests for the Rust packaging) is not in the 413.",
        "",
        "| status | functions | cases | meaning |",
        "|---|---|---|---|",
    ]
    meaning = {
        "ported": "a Rust test (or golden replay of a fixture recorded from Python) covers it",
        "partial": "the pieces are covered, the integrated behaviour is not",
        "dropped": "deliberately not ported, reason given",
        "python-only": "stays a Python test (script, shell, log assertion), reason given",
        "gap": "**nobody covers it** - needs a test or an owner decision",
    }
    for s in order:
        lines.append(f"| {s} | {status_funcs[s]} | {status_cases[s]} | {meaning[s]} |")
    lines.append(f"| **total** | **{total_funcs}** | **{total_cases}** | every row has a status |")
    lines += ["", "## Gaps (explicit)", ""]
    gaps = [r for r in rows if r[3] == "gap"]
    parts = [r for r in rows if r[3] == "partial"]
    groups = collections.defaultdict(list)
    for r in gaps:
        groups[r[0]].append(r)
    for file in sorted(groups):
        lines.append(f"### {file}: {len(groups[file])} gap(s)")
        lines.append("")
        for r in groups[file]:
            near = ", ".join(short(x) for x in r[4])
            lines.append(f"* `{r[1]}` - {r[5]}" + (f" (nearest: {near})" if near else ""))
        lines.append("")
    lines.append(f"### partial: {len(parts)}")
    lines.append("")
    for r in parts:
        lines.append(f"* `{r[0]}::{r[1]}` - {r[5]}")
    glue = sum(1 for r in gaps if r[0] in ("test_settings_api", "test_setup_api", "test_preview")
               or r[1] == "test_usage_preview_draws_with_the_feed_renderer")
    lines += [
        "",
        f"Reading the gaps: {glue} of {len(gaps)} are the **settings/setup bridge commands** (`app/src/commands/*`,",
        "phase 13); the rest is `test_unpacked_program_is_executable` (unix `chmod`, no test on any OS).",
        "The 32 commands exist and their pure parts (schema `clean`, clock view, diff, scan, session,",
        "capabilities) are tested, but the command-level behaviour tests of `test_settings_api.py`,",
        "`test_setup_api.py` and the `usage_preview`/`clawd_pak` parts of `test_preview.py` were deferred in the",
        "phase 13 report ('needs a real ClockApi', B'') and never written. They need a fake clock (testkit",
        "`FakeClockDevice`) behind a `Bridge` - doable headless; recommended before cutover (phase 15).",
        "",
        "## Per-test table",
        "",
        "| Python file | test function | cases | status | Rust test(s) / reason |",
        "|---|---|---|---|---|",
    ]
    for file, name, cases, status, refs, note in rows:
        what = "<br>".join(short(r) + _case(r) for r in refs)
        text = what + (f"<br>_{note}_" if note else "")
        lines.append(f"| {file} | `{name}` | {cases} | {status} | {text} |")
    lines.append("")
    return "\n".join(lines)


def _case(ref: str) -> str:
    m = re.search(r"\[(.*)\]$", ref)
    return f" (scenario `{m.group(1)}`)" if m else ""


if __name__ == "__main__":
    raise SystemExit(main())
