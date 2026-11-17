#!/usr/bin/env python3
"""Summarize scripts/compat/run.sh output dirs as Markdown.

    scripts/compat/summary.py results/        # results/<app>/<scenario>.{json,png,log,exit.json}

One row per app and scenario: how the run ended, how far it got, whether a screenshot
came out, and the first API pomme lacks. Then each app's most-called missing functions
and methods: the work list for making it run.
"""
import json
import sys
from pathlib import Path


def load(p):
    try:
        return json.loads(p.read_text())
    except (OSError, ValueError):
        return None


def method_name(m):
    return f"{'+' if m.get('class_method') else '-'}[{m['class']} {m['selector']}]"


def main(root):
    root = Path(root)
    rows, gaps = [], []
    for app_dir in sorted(p for p in root.iterdir() if p.is_dir()):
        app = app_dir.name
        funcs, methods = {}, {}
        for log in sorted(app_dir.glob("*.log")):
            name = log.stem
            rep = load(app_dir / f"{name}.json")
            ex = load(app_dir / f"{name}.exit.json") or {}
            run = (rep or {}).get("run") or {}
            first = ""
            for f in run.get("missing_functions", []):
                funcs[f["symbol"]] = funcs.get(f["symbol"], 0) + f["count"]
                if f.get("first"):
                    first = f"`{f['symbol']}`"
            for m in run.get("missing_methods", []):
                key = method_name(m)
                methods[key] = methods.get(key, 0) + m["count"]
                if m.get("first"):
                    first = f"`{key}`"
            ended = run.get("ended") or ("no report" if rep is None else "")
            if ex.get("exit") not in (None, 0):
                ended = f"{ended} (exit {ex['exit']})".strip()
            shot = "yes" if (app_dir / f"{name}.png").exists() else "no"
            rows.append(
                f"| {app} | {name} | {ended.replace('|', '/')} | {run.get('instructions', 0) / 1e9:.1f} B "
                f"| {ex.get('wall_seconds', '?')} s | {shot} | {len(run.get('missing_functions', []))} "
                f"| {len(run.get('missing_methods', []))} | {first} |"
            )
        top = sorted(funcs.items(), key=lambda kv: -kv[1])[:10] + sorted(methods.items(), key=lambda kv: -kv[1])[:10]
        if top:
            gaps.append(f"\n<details><summary>{app}: most-called missing APIs</summary>\n")
            gaps += [f"- `{k}` ×{n}" for k, n in top]
            gaps.append("\n</details>")
    print("## App compatibility\n")
    print("| App | Scenario | Ended | Instructions | Wall | Screenshot | Missing functions | Missing methods | First miss |")
    print("| --- | --- | --- | --- | --- | --- | --- | --- | --- |")
    print("\n".join(rows))
    print("\n".join(gaps))


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "results")
