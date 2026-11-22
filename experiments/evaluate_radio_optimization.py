"""Compare two engine probes on radio insertion and unchanged validity behavior."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import signal
import statistics
import subprocess


ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "experiments/e3-servo/fixtures"


def run(binary, fixture, observer, seconds, output, name):
    command = [str(binary), fixture, "--await-reduction", "--measure", "--seconds", str(seconds)]
    if observer:
        command.append("--intersection-observer")
    proc = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            text=True, errors="replace", start_new_session=True)
    timed_out = False
    try:
        log, _ = proc.communicate(timeout=seconds + 15)
    except subprocess.TimeoutExpired:
        timed_out = True
        os.killpg(proc.pid, signal.SIGKILL)
        log, _ = proc.communicate()
    (output / f"{name}.log").write_text(log)
    results = [json.loads(line.removeprefix("reduction: "))
               for line in log.splitlines() if line.startswith("reduction: ")]
    if proc.returncode != 0 or timed_out or len(results) != 1:
        raise RuntimeError(f"{name}: incomplete probe, status={proc.returncode}, timed_out={timed_out}")
    elapsed = results[0].get("milliseconds")
    if not isinstance(elapsed, (int, float)) or not math.isfinite(elapsed) or elapsed <= 0:
        raise RuntimeError(f"{name}: invalid timing result")
    return {"name": name, "command": command, "exit_status": proc.returncode, "result": results[0]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--profile", choices=["debug", "release"], required=True)
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--seconds", type=int, default=60)
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    if args.repeat < 1 or not 21 <= args.seconds <= 300:
        parser.error("--repeat must be positive and --seconds must be 21 to 300")
    binaries = {label: path.resolve(strict=True)
                for label, path in [("baseline", args.baseline), ("candidate", args.candidate)]}
    if os.path.samefile(*binaries.values()):
        parser.error("Baseline and candidate must be separate executables")
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    output = args.out or ROOT / "target/radio-optimization" / f"{timestamp}-{args.profile}"
    output.mkdir(parents=True, exist_ok=False)
    report = {"started_utc": timestamp, "profile": args.profile, "repeat": args.repeat,
              "binaries": {label: {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
                           for label, path in binaries.items()}, "cases": []}

    def retain(case):
        report["cases"].append(case)
        (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps({"name": case["name"], "result": case["result"]}), flush=True)

    validity = {}
    for label, binary in binaries.items():
        case = run(binary, (FIXTURES / "radio-validity.html").as_uri(), False,
                   args.seconds, output, f"{label}-validity")
        retain(case)
        validity[label] = case["result"]
        if case["result"].get("checks") != 220 or not isinstance(case["result"].get("failures"), list):
            raise RuntimeError(f"{case['name']}: incomplete validity fixture")
    same = (validity["baseline"]["checks"] == validity["candidate"]["checks"]
            and validity["baseline"]["failures"] == validity["candidate"]["failures"])
    report["validity_results_unchanged"] = same
    (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    if not same:
        raise RuntimeError("Candidate changed validity results; retained both outcomes")

    fixture = (FIXTURES / "radio-group.html").as_uri()
    for label, binary in binaries.items():
        case = run(binary, fixture + "?type=text", False, args.seconds, output, f"{label}-text")
        if (case["result"].get("nodes"), case["result"].get("radios"), case["result"].get("type")) != (10000, 40, "text"):
            raise RuntimeError(f"Unexpected text control in {case['name']}")
        retain(case)
    for observer in [False, True]:
        setting = "enabled" if observer else "default"
        for iteration in range(args.repeat):
            order = list(binaries) if iteration % 2 == 0 else list(reversed(binaries))
            for label in order:
                case = run(binaries[label], fixture, observer, args.seconds, output,
                           f"{label}-radio-{setting}-{iteration + 1:02d}")
                result = case["result"]
                if (result.get("nodes"), result.get("radios"), result.get("type")) != (10000, 40, "radio"):
                    raise RuntimeError(f"Unexpected workload in {case['name']}: {result}")
                retain(case)
    report["medians_ms"] = {}
    for setting in ["default", "enabled"]:
        values = {}
        for label in binaries:
            values[label] = statistics.median(
                case["result"]["milliseconds"] for case in report["cases"]
                if case["name"].startswith(f"{label}-radio-{setting}-"))
        report["medians_ms"][setting] = {**values, "speedup": values["baseline"] / values["candidate"]}
    report["completed"] = True
    (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"medians_ms": report["medians_ms"], "validity_results_unchanged": same}), flush=True)


if __name__ == "__main__":
    main()
