"""Run the async bridge experiment with a process deadline and CPU measurements."""
import argparse
import os
from pathlib import Path
import resource
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default=str(Path(os.environ.get("CARGO_TARGET_DIR", "target")) /
                                               "debug/bulwark-experiment-e7"))
    parser.add_argument("--servo", action="store_true")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--idle", action="store_true")
    mode.add_argument("--probe-keyword-await", action="store_true")
    parser.add_argument("--timeout", type=float, default=35)
    args = parser.parse_args()
    if args.timeout <= 0:
        parser.error("--timeout must be positive")
    command = [args.binary]
    if args.servo:
        command.append("--servo")
    if args.idle:
        command.append("--idle")
    if args.probe_keyword_await:
        command.append("--await-keyword-probe")
        try:
            result = subprocess.run(command, capture_output=True, text=True,
                                    timeout=min(args.timeout, 3), check=True)
        except subprocess.TimeoutExpired as error:
            output = error.stdout or b""
            if isinstance(output, bytes):
                output = output.decode(errors="replace")
            if "E7-await-keyword-started" not in output:
                raise RuntimeError("Probe timed out before its async function started") from error
            print("REPRODUCED: await under keyword lookup blocked the runtime; child terminated.")
        else:
            print(result.stdout, end="")
            print("Keyword await completed; the blocking gap was not reproduced.")
        return
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    started = time.monotonic()
    subprocess.run(command, timeout=args.timeout, check=True)
    elapsed = time.monotonic() - started
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    cpu = after.ru_utime + after.ru_stime - before.ru_utime - before.ru_stime
    print(f"E7 process: elapsed_seconds={elapsed:.3f}, cpu_seconds={cpu:.3f}, cpu_over_elapsed={cpu / elapsed:.3f}")


if __name__ == "__main__":
    main()
