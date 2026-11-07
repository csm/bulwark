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
    parser.add_argument("--native", action="store_true")
    parser.add_argument("--close-after-ms", type=int)
    parser.add_argument("--nrepl-port-file")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--idle", action="store_true")
    mode.add_argument("--idle-executor", action="store_true")
    mode.add_argument("--idle-baseline", action="store_true")
    mode.add_argument("--probe-keyword-await", action="store_true")
    mode.add_argument("--require-keyword-await", action="store_true")
    mode.add_argument("--interactive", action="store_true")
    mode.add_argument("--fail-runtime", action="store_true")
    mode.add_argument("--panic-runtime", action="store_true")
    mode.add_argument("--unresponsive-runtime", action="store_true")
    mode.add_argument("--nrepl", action="store_true")
    parser.add_argument("--timeout", type=float, default=35)
    args = parser.parse_args()
    if args.timeout <= 0:
        parser.error("--timeout must be positive")
    if (args.interactive or args.fail_runtime or args.panic_runtime or args.unresponsive_runtime or args.close_after_ms is not None) and not args.native:
        parser.error("native lifecycle options require --native")
    if args.close_after_ms is not None and args.close_after_ms <= 0:
        parser.error("--close-after-ms must be positive")
    if args.nrepl_port_file and not args.nrepl:
        parser.error("--nrepl-port-file requires --nrepl")
    command = [args.binary]
    if args.servo:
        command.append("--servo")
    if args.native:
        command.append("--native")
    if args.nrepl:
        command.append("--nrepl")
    if args.nrepl_port_file:
        command.extend(["--nrepl-port-file", args.nrepl_port_file])
    if args.close_after_ms is not None:
        command.extend(["--close-after-ms", str(args.close_after_ms)])
    if args.interactive:
        command.append("--interactive")
    if args.fail_runtime:
        command.append("--fail-runtime")
    if args.panic_runtime:
        command.append("--panic-runtime")
    if args.unresponsive_runtime:
        command.append("--unresponsive-runtime")
    if args.idle:
        command.append("--idle")
    if args.idle_executor:
        command.append("--idle-executor")
    if args.idle_baseline:
        command.append("--idle-baseline")
    if args.probe_keyword_await or args.require_keyword_await:
        command.append("--await-keyword-probe")
        try:
            result = subprocess.run(command, capture_output=True, text=True,
                                    timeout=args.timeout if args.require_keyword_await else min(args.timeout, 3),
                                    check=True)
        except subprocess.TimeoutExpired as error:
            if args.require_keyword_await:
                raise RuntimeError("Async call regression exceeded its process deadline; child terminated") from error
            output = error.stdout or b""
            if isinstance(output, bytes):
                output = output.decode(errors="replace")
            if "E7-await-keyword-started" not in output:
                raise RuntimeError("Probe timed out before its async function started") from error
            print("REPRODUCED: await under keyword lookup blocked the runtime; child terminated.")
        except subprocess.CalledProcessError as error:
            print(error.stdout or "", end="")
            print(error.stderr or "", end="")
            raise
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
