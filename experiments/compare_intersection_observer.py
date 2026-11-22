"""Compare Servo's observer preference in isolated processes with bounded runtimes.

The probes measure diagnostic responsiveness, not physical browsing acceptance.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import queue
import re
import signal
import subprocess
import sys
import threading
import time


SITES = {
    "wikipedia": "https://en.wikipedia.org/wiki/Rust_%28programming_language%29",
    "hn": "https://news.ycombinator.com/",
    "github": "https://github.com/servo/servo",
    "mdn": "https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/map",
    "bbc": "https://www.bbc.com/news",
    "mdn-issue": "https://developer.mozilla.org/en-US/docs/Web",
    "amazon-issue": "https://www.amazon.com/",
}


DEFAULT_SITES = tuple(SITES)
CONFIGURATIONS = {
    "default": [],
    "enabled": ["--intersection-observer"],
    "compatibility": ["--intersection-observer", "--adopted-stylesheets"],
}
RADIO_FIXTURE = (Path(__file__).resolve().parent / "e3-servo/fixtures/radio-group.html").as_uri()
SITES.update({"radio-group": RADIO_FIXTURE, "radio-text": RADIO_FIXTURE + "?type=text"})


def probe(command, output, name, seconds, sample_on_stall, stall_seconds):
    """Retain live output and optionally sample this child after a reply gap."""
    started = time.monotonic()
    deadline = started + seconds + 15
    proc = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            text=True, errors="replace", start_new_session=True)
    messages = queue.Queue()

    def read_output():
        for line in proc.stdout:
            messages.put(line)

    reader = threading.Thread(target=read_output, daemon=True)
    reader.start()
    lines = []
    last_heartbeat = None
    stack_sample = None
    timed_out = False
    with (output / f"{name}.log").open("w") as log:
        def drain():
            nonlocal last_heartbeat
            while True:
                try:
                    line = messages.get_nowait()
                except queue.Empty:
                    break
                lines.append(line)
                log.write(line)
                log.flush()
                if line.startswith("heartbeat:"):
                    last_heartbeat = time.monotonic()

        try:
            while True:
                drain()
                waited, status, usage = os.wait4(proc.pid, os.WNOHANG)
                if waited:
                    proc.returncode = os.waitstatus_to_exitcode(status)
                    break
                if time.monotonic() > deadline:
                    timed_out = True
                    os.killpg(proc.pid, signal.SIGKILL)
                    _, status, usage = os.wait4(proc.pid, 0)
                    proc.returncode = os.waitstatus_to_exitcode(status)
                    break
                if (sample_on_stall and stack_sample is None and last_heartbeat is not None
                        and time.monotonic() - last_heartbeat > stall_seconds):
                    path = output / f"{name}.sample.txt"
                    stack_sample = {"path": str(path),
                                    "reply_gap_seconds": time.monotonic() - last_heartbeat}
                    try:
                        sample = subprocess.run(
                            ["/usr/bin/sample", str(proc.pid), "2", "1", "-file", str(path)],
                            capture_output=True, text=True,
                            timeout=min(12, max(0.1, deadline - time.monotonic())))
                        stack_sample["exit_status"] = sample.returncode
                        (output / f"{name}.sample-command.log").write_text(sample.stdout + sample.stderr)
                    except subprocess.TimeoutExpired:
                        stack_sample["timed_out"] = True
                time.sleep(0.05)
        finally:
            if proc.returncode is None:
                os.killpg(proc.pid, signal.SIGKILL)
                proc.wait()
            reader.join(timeout=2)
            drain()
            proc.stdout.close()
    # wait4 records only the probe, excluding the optional sampling subprocess.
    return {"log": "".join(lines), "exit_status": proc.returncode, "timed_out": timed_out,
            "wall_seconds": time.monotonic() - started,
            "cpu_seconds": usage.ru_utime + usage.ru_stime, "stack_sample": stack_sample}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/examples/bbc_probe"))
    selection = parser.add_mutually_exclusive_group()
    selection.add_argument("--site", action="append", choices=SITES)
    selection.add_argument("--url", help="An explicit URL, including a local reduction fixture")
    parser.add_argument("--repeat", type=int, default=1)
    parser.add_argument("--configuration", action="append", choices=CONFIGURATIONS,
                        help="Preference configurations to compare (default: default and enabled)")
    parser.add_argument("--user-script", type=Path,
                        help="Optional diagnostic script applied equally to every configuration")
    parser.add_argument("--sample-on-stall", action="store_true", help="Capture a macOS thread sample after a reply gap")
    parser.add_argument("--stall-seconds", type=float, default=3)
    parser.add_argument("--seconds", type=int, default=30)
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    if not 21 <= args.seconds <= 300:
        parser.error("--seconds must be 21 to 300")
    if args.repeat < 1 or not 0 < args.stall_seconds < float("inf"):
        parser.error("--repeat and --stall-seconds must be positive")
    if args.sample_on_stall and sys.platform != "darwin":
        parser.error("--sample-on-stall requires macOS /usr/bin/sample")
    binary = args.binary.resolve(strict=True)
    user_script = args.user_script.resolve(strict=True) if args.user_script else None
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    output = args.out or Path("target/observer-comparisons") / timestamp
    output.mkdir(parents=True, exist_ok=False)
    report = {"started_utc": timestamp, "binary": str(binary),
              "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
              "seconds": args.seconds, "repeat": args.repeat,
              "configurations": list(dict.fromkeys(args.configuration or ["default", "enabled"])),
              "user_script": str(user_script) if user_script else None,
              "user_script_sha256": hashlib.sha256(user_script.read_bytes()).hexdigest() if user_script else None,
              "stall_seconds": args.stall_seconds, "cases": []}
    sites = {"custom": args.url} if args.url else {site: SITES[site] for site in args.site or DEFAULT_SITES}
    for iteration in range(args.repeat):
        for site, url in sites.items():
            configurations = report["configurations"]
            # Alternate repeat order to avoid always measuring the candidate last.
            order = configurations if iteration % 2 == 0 else reversed(configurations)
            for configuration in order:
                name = f"{site}-{configuration}"
                if args.repeat > 1:
                    name += f"-{iteration + 1:02d}"
                command = [str(binary), url, "--seconds", str(args.seconds), "--measure"]
                command.extend(CONFIGURATIONS[configuration])
                if user_script:
                    command.extend(["--user-script", str(user_script)])
                result = probe(command, output, name, args.seconds, args.sample_on_stall, args.stall_seconds)
                log = result.pop("log")
                heartbeats = [int(n) for n in re.findall(r"heartbeat: ms=(\d+) ok=true", log)]
                samples = [json.loads(line.removeprefix("inspection: "))
                           for line in log.splitlines() if line.startswith("inspection: ")]
                turns = re.findall(r"longest-turn-ms: (\d+)", log)
                case = {"name": name, "configuration": configuration,
                        "url": url, "command": command, **result,
                        "cpu_per_wall": result["cpu_seconds"] / result["wall_seconds"],
                        "heartbeats": len(heartbeats), "max_heartbeat_ms": max(heartbeats, default=None),
                        "longest_turn_ms": int(turns[-1]) if turns else None, "samples": samples}
                report["cases"].append(case)
                (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
                print(json.dumps({k: v for k, v in case.items() if k not in ("command", "samples")}), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
