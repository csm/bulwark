"""Run the E1 headless driver with a concurrent nREPL client."""
import argparse
import os
from pathlib import Path
import re
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seconds", type=int, default=60)
    parser.add_argument("--code", default="(swap! bulwark.experiment/nrepl-evals inc)")
    parser.add_argument("--binary", default=str(Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "debug/bulwark-experiment-e1"))
    args = parser.parse_args()
    if args.seconds < 3:
        parser.error("--seconds must be at least 3")
    proc = subprocess.Popen(
        [args.binary, "--headless", "--seconds", str(args.seconds)],
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
    )
    try:
        first = proc.stdout.readline()
        print(first, end="", flush=True)
        match = re.search(r"127\.0\.0\.1:(\d+)", first)
        if not match:
            raise RuntimeError("E1 did not report an nREPL listener")
        client = subprocess.run(
            [sys.executable, str(Path(__file__).with_name("nrepl_probe.py")),
             match[1], "--seconds", str(args.seconds - 2), "--code", args.code],
            timeout=args.seconds + 15,
        )
        output, _ = proc.communicate(timeout=15)
        print(output, end="", flush=True)
        if proc.returncode or client.returncode:
            raise SystemExit(proc.returncode or client.returncode)
        match = re.search(r"nrepl_evals=(\d+)", output)
        if not match or int(match[1]) == 0:
            raise RuntimeError("No nREPL evaluations reached the host")
    finally:
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()


if __name__ == "__main__":
    main()
