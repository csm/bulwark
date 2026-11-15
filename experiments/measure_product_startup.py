"""Measure process creation to the first native page/chrome presentation.

The parent timestamps receipt of the presentation marker, so each sample includes
process creation, dynamic loading, and pipe delivery. Profiles start empty. The
filesystem and shader caches are not cleared. Run after the release build ends.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import selectors
import statistics
import subprocess
import tempfile
import time


MARKER = b"Bulwark native: first-content-presented\n"


def sample(binary, profile, timeout):
    started = time.monotonic()
    process = subprocess.Popen(
        [str(binary), "--profile", str(profile), "--close-after-ms", "3000"],
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    output = bytearray()
    elapsed = None
    try:
        with selectors.DefaultSelector() as selector:
            selector.register(process.stdout, selectors.EVENT_READ)
            while True:
                remaining = started + timeout - time.monotonic()
                if remaining <= 0 or not selector.select(remaining):
                    raise RuntimeError("Startup sample exceeded its process deadline")
                chunk = os.read(process.stdout.fileno(), 65536)
                if not chunk:
                    break
                output.extend(chunk)
                if elapsed is None and MARKER in output:
                    elapsed = time.monotonic() - started
        process.wait(timeout=max(0.01, started + timeout - time.monotonic()))
        text = output.decode(errors="replace")
        if (process.returncode != 0 or output.count(MARKER) != 1
                or "shutdown=joined" not in text):
            raise RuntimeError(f"Invalid startup sample (exit {process.returncode}):\n{text}")
        return {"seconds": elapsed, "output": text}
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        process.stdout.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/bulwark-shell"))
    parser.add_argument("--samples", type=int, default=7)
    parser.add_argument("--timeout", type=float, default=30)
    args = parser.parse_args()
    if args.samples <= 0 or args.timeout <= 3:
        parser.error("--samples must be positive and --timeout must exceed three seconds")
    binary = args.binary.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="bulwark-startup-") as directory:
        samples = []
        for index in range(args.samples):
            profile = Path(directory) / str(index)
            profile.mkdir()
            samples.append(sample(binary, profile, args.timeout))
    with binary.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    print(json.dumps({
        "binary": str(binary), "sha256": digest,
        "url": "about:blank", "profile": "new empty directory per sample; no init or nREPL",
        "clock": "parent monotonic, before Popen to marker receipt after page/chrome presentation",
        "samples": samples, "median_seconds": statistics.median(s["seconds"] for s in samples),
    }, indent=2))


if __name__ == "__main__":
    main()
