#!/usr/bin/env python3
"""Repeat complete serial/sharded suites; retain raw logs and executed timings.

No pass/fail performance threshold. RSS is sampled for the whole process tree
(shared pages may be counted more than once), not just the Node supervisor.
"""
import json
from pathlib import Path
import resource
import subprocess
import sys
import time


def tree_rss(root):
    rows = [line.split() for line in subprocess.check_output(
        ["ps", "-axo", "pid=,ppid=,rss="], text=True).splitlines()]
    children = {root}
    while True:
        expanded = children | {int(pid) for pid, parent, _ in rows if int(parent) in children}
        if expanded == children:
            break
        children = expanded
    return sum(int(rss) for pid, _, rss in rows if int(pid) in children)


def main():
    destination = Path(sys.argv[1] if len(sys.argv) > 1 else "target/ci-profile/comparison")
    attempts = int(sys.argv[2]) if len(sys.argv) > 2 else 3
    if attempts < 1:
        raise ValueError("At least one complete comparison is required")
    destination.mkdir(parents=True, exist_ok=True)
    results = []
    for attempt in range(1, attempts + 1):
        for mode in ("serial", "sharded"):
            command = ["npm", "run", "test:browser"] + (["--", "--serial"] if mode == "serial" else [])
            log = destination / f"{mode}-{attempt}.log"
            before = resource.getrusage(resource.RUSAGE_CHILDREN)
            started, peak = time.monotonic(), 0
            with log.open("w") as output:
                child = subprocess.Popen(command, stdout=output, stderr=subprocess.STDOUT)
                while child.poll() is None:
                    peak = max(peak, tree_rss(child.pid))
                    time.sleep(0.5)
            after = resource.getrusage(resource.RUSAGE_CHILDREN)
            row = {"mode": mode, "attempt": attempt, "command": command,
                   "seconds": round(time.monotonic() - started, 3), "exit_code": child.returncode,
                   "sampled_tree_peak_rss_kib": peak,
                   "child_user_seconds": round(after.ru_utime - before.ru_utime, 3),
                   "child_system_seconds": round(after.ru_stime - before.ru_stime, 3)}
            results.append(row)
            (destination / "results.json").write_text(json.dumps(results, indent=2) + "\n")
            print(json.dumps(row), flush=True)
            if child.returncode:
                return child.returncode if child.returncode > 0 else 128 - child.returncode
    return 0


if __name__ == "__main__":
    sys.exit(main())
