"""Assert native E7 wakeups and shutdown outcomes using bounded child processes."""
import argparse
import os
from pathlib import Path
import re
import subprocess
import time


def run_case(binary, name, flags, expected_success, expected_text, timeout):
    started = time.monotonic()
    result = subprocess.run([binary, "--native", *flags], capture_output=True,
                            text=True, timeout=timeout)
    output = result.stdout + result.stderr
    print(output, end="", flush=True)
    if (result.returncode == 0) != expected_success:
        raise RuntimeError(f"{name}: unexpected exit status {result.returncode}")
    for text in expected_text:
        if text not in output:
            raise RuntimeError(f"{name}: missing {text!r}")
    print(f"PASS {name}: process_seconds={time.monotonic() - started:.3f}", flush=True)
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default=str(
        Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "debug/bulwark-experiment-e7"))
    parser.add_argument("--timeout", type=float, default=35)
    args = parser.parse_args()
    if args.timeout <= 0:
        parser.error("--timeout must be positive")

    output = run_case(args.binary, "workflow", [], True,
                      ["reason=Some(Completed)", "shutdown=joined"], args.timeout)
    frames = re.search(r"presented_frames_during_stall=(\d+)", output)
    if not frames or int(frames[1]) <= 0:
        raise RuntimeError("Native rendering stopped during the runtime stall")
    run_case(args.binary, "close-await", ["--idle", "--close-after-ms", "100"], True,
             ["reason=Some(TimedClose)", "shutdown=joined"], args.timeout)
    output = run_case(args.binary, "interactive-commands",
                      ["--interactive", "--close-after-ms", "500"], True,
                      ["reason=Some(TimedClose)", "shutdown=joined"], args.timeout)
    handled = re.search(r"handled=(\d+)", output)
    if not handled or int(handled[1]) < 3:
        raise RuntimeError("Interactive program did not update its native page")
    run_case(args.binary, "runtime-error", ["--fail-runtime"], False,
             ["shutdown=joined", "Injected runtime failure"], args.timeout)
    run_case(args.binary, "runtime-panic", ["--panic-runtime"], False,
             ["shutdown=joined", "Runtime thread panicked"], args.timeout)
    run_case(args.binary, "unresponsive-native-call",
             ["--unresponsive-runtime", "--close-after-ms", "100"], False,
             ["shutdown=unjoined", "Runtime did not stop within two seconds"], args.timeout)


if __name__ == "__main__":
    main()
