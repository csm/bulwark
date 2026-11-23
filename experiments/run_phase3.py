"""Run P3.1 mechanism probes with independent process deadlines and retained logs."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    target = Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "debug"
    parser.add_argument("--binary", type=Path, default=target / "examples/phase3")
    parser.add_argument("--e4-binary", type=Path, default=target / "bulwark-experiment-e4")
    parser.add_argument("--runtime-only", action="store_true")
    parser.add_argument("--output", type=Path, default=Path("target/phase3-evidence"))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    cases = [
        ("authority-tiered", [str(args.binary), "--runtime"]),
        ("authority-treewalk", [str(args.binary), "--runtime", "--interpreter"]),
    ]
    if not args.runtime_only:
        cases += [
            ("dom-tiered", [str(args.binary)]),
            ("dom-treewalk", [str(args.binary), "--interpreter"]),
            ("servo-contracts", [str(args.e4_binary), "--phase3"]),
            ("e4-baseline", [str(args.e4_binary)]),
        ]
    results = []
    for name, command in cases:
        started = time.monotonic()
        with (args.output / f"{name}.log").open("w") as log:
            try:
                child = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, timeout=35)
                status = child.returncode
            except subprocess.TimeoutExpired:
                status = "timeout"
        result = {"name": name, "command": command, "status": status,
                  "elapsed_seconds": time.monotonic() - started}
        results.append(result)
        (args.output / "results.json").write_text(json.dumps(results, indent=2) + "\n")
        print(f"{name}: {status} ({result['elapsed_seconds']:.3f}s)", flush=True)
    if any(result["status"] != 0 for result in results):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
