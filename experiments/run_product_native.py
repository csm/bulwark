"""Run process-bounded checks of the product native host and runtime lifecycle."""
import argparse
import json
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
    parser.add_argument("--artifacts-dir", type=Path,
                        help="Retain page/chrome, mixed-pane, and minibuffer PNGs in this directory")
    args = parser.parse_args()
    if args.timeout <= 0:
        parser.error("--timeout must be positive")
    if args.artifacts_dir:
        args.artifacts_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="bulwark-product-native-") as directory:
        directory = Path(directory)
        root = Path(__file__).resolve().parent.parent
        artifacts = args.artifacts_dir or directory
        suffix = "interpreter" if args.interpreter else "tiered"
        screenshot = artifacts / f"page-and-chrome-{suffix}.png"
        mode = ["--interpreter"] if args.interpreter else []
        run_case(args.binary, "page-and-chrome", [*mode, "--native-smoke", "--screenshot", str(screenshot)],
                 True, ["Phase 1 native passed"], args.timeout)
        image = screenshot.read_bytes()
        if image[:8] != b"\x89PNG\r\n\x1a\n" or min(struct.unpack(">II", image[16:24])) <= 0:
            raise RuntimeError("Native composed frame is not a nonempty PNG")
        run_case(args.binary, "close-parked-runtime", [*mode, "--close-after-ms", "250"],
                 True, ["window-ready"], args.timeout)
        profile = directory / "profile"
        profile.mkdir()
        init = profile / "init.cljrs"
        # Default startup loads the selected profile without opening an nREPL listener.
        run_case(args.binary, "missing-init", [*mode, "--profile", str(profile), "--close-after-ms", "1000"],
                 True, ["window-ready"], args.timeout)
        init.write_text('(await (bulwark.native/log "Init after defaults"))\n'
                        '(when-not (:command (bulwark.keymap/lookup "C-x b")) (throw (ex-info "No defaults" {})))\n')
        run_case(args.binary, "profile-without-nrepl", [*mode, "--profile", str(profile), "--close-after-ms", "1500"],
                 True, ["Init after defaults"], args.timeout)
        init.write_text('(bulwark.hooks/add-hook! :buffer/created :broken-init-check\n'
                        ' (fn [event] (when (= :web (:buffer/kind (bulwark.buffer/buffer (:buffer event))))\n'
                        '  (when-not (and (= :error (:status @bulwark.config/last-result))\n'
                        '    (clojure.string/includes? (:buffer/text (bulwark.buffer/find-named "*Messages*")) "Startup init failure"))\n'
                        '   (throw (ex-info "Missing startup diagnostic" {})))\n'
                        '  (bulwark.native/log "Broken init left browser usable"))))\n'
                        '(throw (ex-info "Startup init failure" {}))\n')
        run_case(args.binary, "broken-init", [*mode, "--profile", str(profile), "--close-after-ms", "1500"],
                 True, ["Broken init left browser usable"], args.timeout)
        if (profile / ".nrepl-port").exists():
            raise RuntimeError("Profile selection unexpectedly enabled nREPL")
        init.unlink()
        config_preamble = (f"(def test-profile {json.dumps(str(profile))})\n"
                           f"(def example-init-source {json.dumps((root / 'examples/init.cljrs').read_text())})\n")
        programs = [
            ("default-browsing", (root / "clj/tests/browsing-native.cljrs").read_text(),
             True, ["Default browsing and internal REPL assertions passed", "first-content-presented"]),
            ("configuration", config_preamble + (root / "clj/tests/config-native.cljrs").read_text(),
             True, ["Native configuration assertions passed"]),
            ("retained-views", "(def retained-fixture-url user/initial-url)\n" +
             (root / "clj/tests/retained-views.cljrs").read_text() +
             "\n(bulwark.tests.retained-views/retained-views!)",
             True, ["Retained webview assertions passed"]),
            ("retained-buffers", (root / "clj/tests/buffers-native.cljrs").read_text(),
             True, ["Retained buffer assertions passed"]),
            ("mixed-windows", (root / "clj/tests/windows-native.cljrs").read_text(),
             True, ["Native mixed-window assertions passed"]),
            ("minibuffer", (root / "clj/tests/minibuffer-native.cljrs").read_text(),
             True, ["Native minibuffer assertions passed"]),
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
            if name in {"mixed-windows", "minibuffer", "configuration", "default-browsing"}:
                flags += ["--screenshot", str(artifacts / f"{name}-{suffix}.png")]
            if name in {"clojure-browser", "retained-views", "retained-buffers", "mixed-windows", "minibuffer", "configuration", "default-browsing"}:
                flags.append((root / "clj/tests/fixtures/servo.html").as_uri())
            run_case(args.binary, name, flags, success, expected, args.timeout)


if __name__ == "__main__":
    main()
