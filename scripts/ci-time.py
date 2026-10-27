#!/usr/bin/env python3
"""Time an unchanged command, retaining output and nonzero exit status.

Cargo's ordinary test harness prints the duration of each test binary. Preserve
those lines and annotate their binary names without nightly Rust or a new runner.
"""
import json
from pathlib import Path
import subprocess
import sys
import time


def main():
    label, *command = sys.argv[1:]
    cargo_tests = len(command) > 1 and Path(command[0]).name == "cargo" and command[1] == "test"
    started = time.monotonic()
    code = 1
    binary = None
    try:
        with subprocess.Popen(command, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT, text=True, bufsize=1) as child:
            for line in child.stdout:
                print(line, end="", flush=True)
                if cargo_tests and line.lstrip().startswith(("Running ", "Doc-tests ")):
                    binary = line.strip()
                if cargo_tests and line.startswith("test result:"):
                    print("NATIVE_TIMING " + json.dumps({"binary": binary, "result": line.strip()}), flush=True)
            code = child.wait()
    finally:
        print("CI_TIMING " + json.dumps({"stage": label,
              "seconds": round(time.monotonic() - started, 3), "exit_code": code}), flush=True)
    return code if code >= 0 else 128 - code


if __name__ == "__main__":
    sys.exit(main())
