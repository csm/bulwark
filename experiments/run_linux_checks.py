"""Run the Linux validation suites with persistent logs and a small build budget."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import shlex
import shutil
import subprocess
import sys
import time


def commands(suite):
    portable = [
        ["cargo", "fmt", "--all", "--", "--check"],
        ["cargo", "clippy", "--locked", "--workspace", "--all-targets", "--no-default-features", "--", "-D", "warnings"],
        ["cargo", "test", "--locked", "--workspace", "--no-default-features"],
        ["cargo", "build", "--locked", "--no-default-features", "-p", "bulwark-shell", "-p", "bulwark-experiment-e1", "-p", "bulwark-experiment-e7"],
        ["cargo", "run", "--locked", "--no-default-features", "--", "--headless-smoke"],
        ["cargo", "run", "--locked", "--no-default-features", "--", "--interpreter", "--headless-smoke"],
        ["python3", "experiments/run_product_nrepl.py"],
        ["python3", "experiments/run_product_nrepl.py", "--interpreter"],
        ["cargo", "run", "--locked", "-p", "bulwark-experiment-e2"],
        ["python3", "experiments/run_e1.py", "--seconds", "5"],
        ["cargo", "run", "--locked", "-p", "bulwark-experiment-e6"],
        ["python3", "experiments/run_e6.py"],
        ["python3", "experiments/run_e7.py"],
        ["python3", "experiments/run_e7.py", "--require-keyword-await"],
        ["cargo", "run", "--locked", "-p", "bulwark-experiment-e7", "--example", "gc_idle", "--", "--baseline", "--require-idle"],
        ["cargo", "run", "--locked", "-p", "bulwark-experiment-e7", "--example", "gc_idle", "--", "--require-idle"],
        ["python3", "experiments/run_e7_nrepl.py"],
    ]
    servo = [
        ["cargo", "clippy", "--locked", "-p", "bulwark-shell", "-p", "bulwark-engine-servo", "--features", "native,servo", "--all-targets", "--", "-D", "warnings"],
        ["xvfb-run", "-a", "cargo", "test", "--locked", "-p", "bulwark-shell", "-p", "bulwark-engine-servo", "--features", "native,servo"],
        ["xvfb-run", "-a", "cargo", "run", "--locked", "--", "--servo-smoke"],
        ["xvfb-run", "-a", "cargo", "run", "--locked", "--", "--interpreter", "--servo-smoke"],
        ["xvfb-run", "-a", "cargo", "run", "--locked", "--", "--close-after-ms", "1000"],
        ["python3", "experiments/run_product_native.py"],
        ["python3", "experiments/run_product_native.py", "--interpreter"],
        ["python3", "experiments/run_product_nrepl.py", "--native"],
        ["python3", "experiments/run_product_nrepl.py", "--native", "--interpreter"],
    ]
    # Python native process runners need the same virtual display as the shell.
    servo = [["xvfb-run", "-a", *command] if command[0] == "python3" else command
             for command in servo]
    groups = {"portable": portable, "servo": servo,
              "aot": [["python3", "experiments/run_shell_aot.py"]]}
    return [command for group in (groups if suite == "all" else [suite])
            for command in groups[group]]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--suite", choices=["portable", "servo", "aot", "all"], default="portable")
    parser.add_argument("--jobs", type=int, default=1)
    parser.add_argument("--timeout", type=float, default=3600, help="Deadline per command, in seconds")
    parser.add_argument("--report-dir", type=Path)
    parser.add_argument("--list", action="store_true", help="Print commands without running them")
    args = parser.parse_args()
    if args.jobs < 1 or args.timeout <= 0:
        parser.error("--jobs and --timeout must be positive")
    steps = commands(args.suite)
    if args.list:
        print("\n".join(shlex.join(command) for command in steps))
        return 0
    if sys.platform != "linux":
        parser.error("Run this script on the Linux test host")
    root = Path(__file__).resolve().parent.parent
    environment = os.environ.copy()
    environment["PATH"] = str(Path.home() / ".cargo/bin") + os.pathsep + environment.get("PATH", "")
    environment["CARGO_BUILD_JOBS"] = str(args.jobs)
    environment["CARGO_PROFILE_DEV_DEBUG"] = "0"
    environment["CARGO_PROFILE_TEST_DEBUG"] = "0"
    environment["CARGO_INCREMENTAL"] = "0"
    environment.setdefault("RUST_TEST_THREADS", "1")
    environment.setdefault("RUST_TEST_NOCAPTURE", "1")
    environment.setdefault("BULWARK_TEST_PROGRAM_TIMEOUT_SECS", "60")
    environment["LIBGL_ALWAYS_SOFTWARE"] = "1"
    for executable in {command[0] for command in steps}:
        if shutil.which(executable, path=environment["PATH"]) is None:
            parser.error(f"Missing {executable}. See docs/linux-testing.md for prerequisites")
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    report_dir = args.report_dir or root / "target/linux-runs" / f"{timestamp}-{os.getpid()}"
    report_dir = report_dir.resolve()
    # Never mix two runs or overwrite earlier evidence.
    report_dir.mkdir(parents=True, exist_ok=False)
    revision = subprocess.run(["git", "rev-parse", "HEAD"], cwd=root,
                              capture_output=True, text=True, check=True).stdout.strip()
    worktree = subprocess.run(["git", "status", "--porcelain"], cwd=root,
                             capture_output=True, text=True, check=True).stdout
    report = {"revision": revision, "suite": args.suite, "started_utc": timestamp,
              "platform": platform.platform(), "jobs": args.jobs,
              "rust_test_threads": environment["RUST_TEST_THREADS"], "worktree_status": worktree,
              "clojure_program_timeout_secs": environment["BULWARK_TEST_PROGRAM_TIMEOUT_SECS"],
              "runner_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "steps": [], "status": "running"}

    def save():
        temporary = report_dir / "summary.json.tmp"
        temporary.write_text(json.dumps(report, indent=2) + "\n")
        temporary.replace(report_dir / "summary.json")

    save()
    print(f"REPORT {report_dir}", flush=True)
    for index, command in enumerate(steps, 1):
        logfile = report_dir / f"{index:02d}.log"
        print(f"START {index}/{len(steps)} {shlex.join(command)}", flush=True)
        started = time.monotonic()
        timed_out = False
        with logfile.open("wb") as output:
            process = subprocess.Popen(command, cwd=root, env=environment, stdout=output,
                                       stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL,
                                       start_new_session=True)
            try:
                code = process.wait(timeout=args.timeout)
            except subprocess.TimeoutExpired:
                import signal
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                process.wait()
                code = 124
                timed_out = True
        step = {"command": command, "exit_code": code, "seconds": round(time.monotonic() - started, 3),
                "log": logfile.name, "timed_out": timed_out}
        report["steps"].append(step)
        if code:
            report["status"] = "failed"
            save()
            print(f"FAIL {index} exit={code}: {logfile}", flush=True)
            print(logfile.read_text(errors="replace")[-12000:], flush=True)
            return 1
        save()
        print(f"PASS {index} ({step['seconds']}s)", flush=True)
    report["status"] = "passed"
    save()
    print(f"PASS {args.suite}: {len(steps)} checks; {report_dir}", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
