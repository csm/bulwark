"""Run process-bounded checks of the product native host and runtime lifecycle."""
import argparse
import os
from pathlib import Path
import struct
import subprocess
import tempfile


def run_case(binary, name, flags, success, expected, timeout):
    try:
        result = subprocess.run([str(binary), *flags], capture_output=True,
                                text=True, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        # TimeoutExpired retains bytes even with text=True. Keep the last phase
        # and runtime diagnostics in CI instead of discarding captured output.
        for output in (error.stdout, error.stderr):
            if output:
                print(output.decode(errors="replace") if isinstance(output, bytes) else output,
                      end="", flush=True)
        raise RuntimeError(f"{name}: exceeded {timeout}-second process deadline") from error
    output = result.stdout + result.stderr
    print(output, end="", flush=True)
    if (result.returncode == 0) != success:
        raise RuntimeError(f"{name}: unexpected exit status {result.returncode}")
    for text in ["shutdown=joined", *expected]:
        if text not in output:
            raise RuntimeError(f"{name}: missing {text!r}")
    print(f"PASS {name}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path(
        os.environ.get("CARGO_TARGET_DIR", "target")) / "debug/bulwark-shell")
    parser.add_argument("--interpreter", action="store_true")
    parser.add_argument("--timeout", type=float, default=35)
    args = parser.parse_args()
    if args.timeout <= 0:
        parser.error("--timeout must be positive")
    with tempfile.TemporaryDirectory(prefix="bulwark-product-native-") as directory:
        directory = Path(directory)
        root = Path(__file__).resolve().parent.parent
        screenshot = directory / "smoke.png"
        mode = ["--interpreter"] if args.interpreter else []
        run_case(args.binary, "page-and-chrome", [*mode, "--native-smoke", "--screenshot", str(screenshot)],
                 True, ["Phase 1 native passed"], args.timeout)
        image = screenshot.read_bytes()
        if image[:8] != b"\x89PNG\r\n\x1a\n" or min(struct.unpack(">II", image[16:24])) <= 0:
            raise RuntimeError("Native composed frame is not a nonempty PNG")
        run_case(args.binary, "close-parked-runtime", [*mode, "--close-after-ms", "250"],
                 True, ["window-ready"], args.timeout)
        programs = [
            ("clojure-browser", (root / "clj/tests/browser-native.cljrs").read_text(),
             True, ["Native Clojure commands and history passed"]),
            ("hook-failure", '''
                (bulwark.hooks/add-hook! :page/url-changed :failure
                  (fn [event] (throw (ex-info "Injected product hook failure" {}))))
                (defn ^:async program! []
                  (await (bulwark.native/webview-create "about:blank"))
                  (await (clojure.core.async/timeout 60000)))
                (program!)''', False, ["Injected product hook failure"]),
            ("runtime-failure", '(throw (ex-info "Injected product runtime failure" {}))',
             False, ["Injected product runtime failure"]),
            ("dispatcher-failure", '''
                (in-ns 'bulwark.core)
                (defn dispatch! [event] (throw (ex-info "Injected product dispatcher failure" {})))
                (defn ^:async program! []
                  (await (bulwark.native/webview-create "about:blank"))
                  (await (clojure.core.async/timeout 60000)))
                (program!)''', False, ["Injected product dispatcher failure"]),
            ("native-quit", '''
                (defn ^:async program! []
                  (await (bulwark.native/quit))
                  (await (clojure.core.async/timeout 60000)))
                (program!)''', True, ["window-ready"]),
        ]
        for name, source, success, expected in programs:
            script = directory / f"{name}.cljrs"
            script.write_text(source)
            flags = [*mode, "--script", str(script)]
            if name == "clojure-browser":
                flags.append((root / "clj/tests/fixtures/servo.html").as_uri())
            run_case(args.binary, name, flags, success, expected, args.timeout)


if __name__ == "__main__":
    main()
