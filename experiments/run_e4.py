"""Run the Servo DOM channel probe with an independent process deadline."""
import argparse
import os
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default=str(
        Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "debug/bulwark-experiment-e4"))
    parser.add_argument("--timeout", type=float, default=35)
    args = parser.parse_args()
    if args.timeout <= 0:
        parser.error("--timeout must be positive")
    subprocess.run([args.binary], timeout=args.timeout, check=True)


if __name__ == "__main__":
    main()
