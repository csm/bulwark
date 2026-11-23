"""Run the isolated S1 regressions and existing E4 controls with retained evidence."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import platform
import subprocess
import time


ROOT = Path(__file__).resolve().parent.parent


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--preparation", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repeat", type=int, default=3)
    args = parser.parse_args()
    if not 1 <= args.repeat <= 20:
        parser.error("--repeat must be between 1 and 20")
    binary = args.binary.resolve(strict=True)
    provenance = json.loads((args.preparation / "provenance.json").read_text())
    patch = ROOT / "experiments/phase3/patches/servo-0.7-document-scope.patch"
    if provenance["patch_sha256"] != sha256(patch):
        parser.error("The preparation does not match the current patch")
    args.output.mkdir(parents=True, exist_ok=False)
    report = {
        "started_utc": datetime.now(timezone.utc).isoformat(),
        "binary_sha256": sha256(binary), "preparation": provenance,
        "platform": {"os": platform.platform(), "machine": platform.machine()},
        "source_hashes": {name: sha256(ROOT / name) for name in (
            "experiments/e4-dom-channel/Cargo.toml",
            "experiments/e4-dom-channel/src/document_scope.rs",
            "experiments/e4-dom-channel/src/main.rs",
            "experiments/e4-dom-channel/src/contracts.rs",
            "experiments/e4-dom-channel/agent.js",
            "experiments/e4-dom-channel/phase3-agent.js",
            "experiments/prepare_document_scope.py", "experiments/run_document_scope.py",
        )},
        "scope": "Experimental engine only. S1 lifecycle and host delivery remain open.",
        "cases": [],
    }
    cases = [(f"document-scope-{i + 1}", ["--document-scope"])
             for i in range(args.repeat)]
    cases += [("servo-contracts", ["--phase3"]), ("e4-baseline", [])]
    for name, flags in cases:
        started = time.monotonic()
        command = [str(binary), *flags]
        log_path = args.output / f"{name}.log"
        with log_path.open("w") as log:
            try:
                status = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT,
                                        timeout=35).returncode
            except subprocess.TimeoutExpired:
                status = "timeout"
        log = log_path.read_text(errors="replace")
        markers = [line for line in log.splitlines() if line.startswith("S1 ")]
        # An unpatched E4 binary must not pass by silently running its baseline path.
        assertions_complete = flags != ["--document-scope"] or (
            len(markers) == 10 and markers[-1].startswith("S1 document-scope prototype: PASS"))
        report["cases"].append({
            "name": name, "command": command, "status": status,
            "elapsed_seconds": time.monotonic() - started,
            "assertions_complete": assertions_complete, "markers": markers,
            "log_sha256": sha256(log_path),
        })
        (args.output / "results.json").write_text(json.dumps(report, indent=2) + "\n")
        print(f"{name}: {status}, assertions_complete={assertions_complete}", flush=True)
    if any(case["status"] != 0 or not case["assertions_complete"] for case in report["cases"]):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
