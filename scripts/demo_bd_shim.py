#!/usr/bin/env python3
"""Fake `bd` shim used by scripts/record-demo.sh.

Pretends to be the `bd` CLI. Advances a scripted epic state machine each
time `bd graph <id> --json` is called, so the watcher sees progress over
the recording window.

All data is synthetic — no real project names, no real PR numbers.
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

EPIC_ID = "demo-s04"
EPIC_TITLE = "Checkout v2 redesign"

TASKS = [
    (1, "Add config + env overrides",      "task", 101, [],       0),
    (2, "Port payment wire types",         "task", 102, [],       0),
    (3, "Update checkout UI copy",         "task", 103, [],       0),
    (4, "Refactor cart reducer",           "task", 104, [2],      1),
    (5, "Wire payment provider adapter",   "task", None, [4],     2),
    (6, "Add fraud check hook",            "task", None, [1, 4],  2),
    (7, "Add latency + error metrics",     "task", None, [5, 6],  3),
    (8, "Ramp to stage behind flag",       "task", None, [7],     4),
    (9, "Write e2e test suite",            "test", None, [3, 8],  5),
]

# Per-frame status overrides: {frame: {task_n: status}}. Applied cumulatively.
TRANSITIONS: list[dict[int, str]] = [
    {},                           # 0: initial (all open)
    {1: "in_progress"},           # 1
    {2: "in_progress"},           # 2
    {3: "in_progress"},           # 3
    {1: "closed"},                # 4
    {4: "in_progress"},           # 5
    {2: "closed"},                # 6
    {3: "closed"},                # 7
    {4: "closed"},                # 8
    {5: "in_progress"},           # 9
]

STATE_FILE_ENV = "DEMO_BD_STATE_FILE"


def current_frame() -> int:
    """Read and advance the frame counter. Returns the frame for THIS call."""
    path = Path(os.environ[STATE_FILE_ENV])
    if not path.exists():
        path.write_text("0")
        return 0
    n = int(path.read_text().strip() or "0")
    path.write_text(str(n + 1))
    return min(n + 1, len(TRANSITIONS) - 1)


def iso(ts: str) -> str:
    return ts


def build_epic(frame: int) -> dict:
    statuses = {n: "open" for (n, *_) in TASKS}
    for f in range(frame + 1):
        statuses.update(TRANSITIONS[f])

    created_at = "2026-04-18T12:00:00Z"
    updated_at = "2026-04-20T21:51:52Z"

    root = {
        "id": EPIC_ID,
        "title": EPIC_TITLE,
        "description": "Migrate checkout to the v2 pipeline behind a flag. "
                       "Dark-traffic first; no production consumers this phase.",
        "status": "open",
        "priority": 2,
        "issue_type": "epic",
        "owner": "demo",
        "created_at": created_at,
        "updated_at": updated_at,
    }

    issues = [root.copy()]
    for n, title, issue_type, pr, _deps, _layer in TASKS:
        issue = {
            "id": f"{EPIC_ID}.{n}",
            "title": title,
            "description": "",
            "status": statuses[n],
            "priority": 2,
            "issue_type": issue_type,
            "owner": "demo",
            "created_at": created_at,
            "updated_at": updated_at,
        }
        # Only attach the PR ref once the task has started.
        if pr is not None and statuses[n] in ("in_progress", "closed"):
            issue["external_ref"] = f"gh-{pr}"
        issues.append(issue)

    # Build layout.Nodes with DependsOn edges so the watcher can infer layers.
    nodes: dict[str, dict] = {EPIC_ID: {"Issue": root, "DependsOn": []}}
    for n, *_rest in TASKS:
        nid = f"{EPIC_ID}.{n}"
        nodes[nid] = {
            "Issue": next(i for i in issues if i["id"] == nid),
            "DependsOn": [],
        }
    for n, _t, _ty, _pr, deps, _layer in TASKS:
        nid = f"{EPIC_ID}.{n}"
        nodes[nid]["DependsOn"] = [f"{EPIC_ID}.{d}" for d in deps]

    return {
        "root": root,
        "issues": issues,
        "layout": {"Nodes": nodes},
    }


def handle_graph(args: list[str]) -> None:
    # Expected shapes:
    #   bd graph demo-s04 --json --readonly
    #   bd graph --all --json --readonly
    if "--all" in args:
        # No other epics — emit an empty list so the all-epics view is empty.
        print("[]")
        return
    frame = current_frame()
    print(json.dumps(build_epic(frame)))


def main() -> int:
    argv = sys.argv[1:]
    if not argv:
        print("bd-shim: no command", file=sys.stderr)
        return 1
    cmd = argv[0]
    if cmd == "--version":
        print("bd demo-shim 0.0.0")
        return 0
    if cmd == "graph":
        handle_graph(argv[1:])
        return 0
    # Unknown subcommand — succeed silently to keep the watcher happy.
    return 0


if __name__ == "__main__":
    sys.exit(main())
