"""Check opt-in product nREPL, bounded admission, live browser code, and cleanup."""
import argparse
from contextlib import ExitStack
import json
import os
from pathlib import Path
import queue
import re
import socket
import subprocess
import tempfile
import threading
import time

from run_e7_nrepl import Client, interrupt, overload_probe, parked_eval
from nrepl_probe import decode, encode


NS = "bulwark.test.nrepl"
ROOT = Path(__file__).resolve().parent.parent


def probe(a, b, controller, native):
    result = overload_probe(a, b, controller, namespace=NS)
    parked_eval(a, "parked")
    a.send("queued", op="eval", code="(+ 20 22)")
    b.expect(f'({NS}/eval-js! "\\\"parked-peer\\\"")', '"parked-peer"')
    result["parked_interrupt_ms"] = interrupt(controller, a, "parked")
    if not any(reply.get("value") == "42" for reply in a.collect("queued")):
        raise RuntimeError("Session did not recover after interruption")

    before = int(b.evaluate(f"@{NS}/ticks"))
    a.send("busy", op="eval", code='(do (println "product-running") (loop [n 0] (recur (inc n))))')
    a.started("busy", "product-running")
    b.expect(f'({NS}/eval-js! "\\\"cpu-peer\\\"")', '"cpu-peer"')
    b.evaluate("(Thread/sleep 40)")
    if int(b.evaluate(f"@{NS}/ticks")) <= before:
        raise RuntimeError("Busy evaluation starved the product program")
    result["busy_interrupt_ms"] = interrupt(controller, a, "busy")
    a.expect("(+ 20 22)", "42")

    count = 20
    a.socket.sendall(b"".join(encode({
        "id": f"burst-{index}", "op": "eval", "session": a.session,
        "code": f"(do (Thread/sleep 5) (swap! {NS}/order conj {index}) {index})",
    }) for index in range(count)))
    b.expect(f'({NS}/eval-js! "\\\"burst-peer\\\"")', '"burst-peer"')
    for index in range(count):
        if not any(reply.get("value") == str(index) for reply in a.collect(f"burst-{index}")):
            raise RuntimeError(f"Wrong ordered reply {index}")
    b.expect(f"@{NS}/order", "[" + " ".join(map(str, range(count))) + "]")

    # Install a real product hook and command over nREPL, then redefine its Var.
    b.evaluate('''
      (def observed-titles (atom []))
      (def command-result (atom nil))
      (bulwark.hooks/add-hook! :page/title-changed :nrepl-test
        (fn [event]
          (swap! observed-titles conj (:title event))
          (bulwark.native/set-echo! (:title event))))
      (bulwark.commands/defcommand repl-label "REPL test command" [] (reset! command-result :before))
      (bulwark.keymap/bind-key! "C-c t" 'repl-label)''')
    a.expect("(repl-label)", ":before")
    b.evaluate('(bulwark.commands/defcommand repl-label "Redefined command" [] (reset! command-result :after))')
    a.expect('(await (bulwark.keymap/dispatch-key! {:key "c" :control true :pressed true :webview 1}))', ":prefix")
    a.expect('(await (bulwark.keymap/dispatch-key! {:key "t" :pressed true :webview 1}))', ":command")
    a.expect("@command-result", ":after")
    url = ((ROOT / "clj/tests/fixtures/servo.html").as_uri()
           if native else "https://product.test/nrepl")
    b.evaluate(f'(await (bulwark.browser/navigate! "{url}"))')
    until = time.monotonic() + 10
    while b.evaluate("(count @observed-titles)") == "0":
        if time.monotonic() >= until:
            raise RuntimeError("REPL-installed hook did not receive a title")
        time.sleep(0.01)
    b.expect('(get (bulwark.shell/value! (await (bulwark.native/webview-info '
             '@bulwark.browser/active-view))) :url)', json.dumps(url))
    return {**result, "ordered_burst": count, "live_commands_and_hooks": True,
            "cross_session_browser_calls": True}


def run_case(args, case):
    with tempfile.TemporaryDirectory(prefix="bulwark-product-nrepl-") as directory:
        profile = Path(directory) / "profile"
        port_file = profile / ".nrepl-port"
        command = [str(args.binary), "--nrepl", "--profile", str(profile)]
        if args.native:
            command += ["--script", str(ROOT / "clj/tests/nrepl.cljrs")]
            if case == "close":
                command += ["--close-after-ms", "3000"]
        else:
            command += ["--nrepl-smoke"]
        proc = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                text=True, errors="replace")
        lines = []
        messages = queue.Queue()

        def read_output():
            for line in proc.stdout:
                lines.append(line)
                messages.put(line)
            messages.put(None)

        reader = threading.Thread(target=read_output, daemon=True)
        reader.start()
        expired = threading.Event()

        def deadline():
            expired.set()
            proc.kill()

        watchdog = threading.Timer(args.timeout, deadline)
        watchdog.start()
        try:
            port = None
            while port is None:
                line = messages.get(timeout=args.timeout)
                if line is None:
                    raise RuntimeError("Product exited before reporting nREPL")
                match = re.fullmatch(r"nREPL: 127\.0\.0\.1:([1-9]\d*)\s*", line)
                if match:
                    port = int(match[1])
            if port_file.read_text().strip() != str(port):
                raise RuntimeError("Profile port file disagrees with listener")
            with ExitStack() as stack:
                a, b, controller = [Client(stack, port) for _ in range(3)]
                while b.evaluate(f"@{NS}/ready") != "true":
                    time.sleep(0.01)
                result = {}
                if case == "normal":
                    # A second process must leave the running profile untouched.
                    collision = subprocess.run(
                        [str(args.binary), "--nrepl-smoke", "--nrepl", "--profile", str(profile)],
                        capture_output=True, text=True, timeout=10)
                    if collision.returncode == 0 or "Cannot reserve" not in collision.stderr:
                        raise RuntimeError(f"Duplicate profile accepted: {collision}")
                    if port_file.read_text().strip() != str(port):
                        raise RuntimeError("Duplicate process changed the live port file")
                    result = probe(a, b, controller, args.native)
                parked_eval(a, "shutdown-parked")
                if case == "normal":
                    b.send("stop", op="eval", code=f"(reset! {NS}/running false)")
                elif case == "runtime-error":
                    b.send("stop", op="eval", code=f"(reset! {NS}/running :fail)")
                elif case == "dispatcher-error":
                    fixture = (ROOT / "clj/tests/fixtures/servo.html").as_uri()
                    b.send("stop", op="eval", code='''
                      (bulwark.hooks/add-hook! :page/title-changed :failure
                        (fn [event] (throw (ex-info "Injected nREPL hook failure" {}))))
                      (await (bulwark.browser/navigate! "''' + fixture + '''"))''')
                proc.wait(timeout=args.timeout)
                reader.join(timeout=2)
                failure = case in ("runtime-error", "dispatcher-error")
                if expired.is_set() or (proc.returncode == 0) == failure:
                    raise RuntimeError(f"Wrong exit for {case}: {proc.returncode}")
                try:
                    while True:
                        decode(a.stream)
                except (EOFError, ConnectionError):
                    pass
                if port_file.exists():
                    raise RuntimeError("Port file survived shutdown")
                try:
                    connection = socket.create_connection(("127.0.0.1", port), timeout=1)
                except ConnectionError:
                    pass
                else:
                    connection.close()
                    raise RuntimeError("Listener survived shutdown")
            output = "".join(lines)
            limits = re.search(r"max_jobs=(\d+), max_pending=(\d+), budget_exhausted=(\d+)", output)
            if not limits or int(limits[1]) > 4 or int(limits[2]) > 32:
                raise RuntimeError("Missing or invalid admission measurements")
            if case == "normal" and int(limits[3]) == 0:
                raise RuntimeError("Probe did not exercise budget exhaustion")
            if not failure or args.native:
                if "shutdown=joined" not in output:
                    raise RuntimeError("Host did not join its runtime")
            if failure and "Injected nREPL" not in output:
                raise RuntimeError("Expected failure did not propagate to host")
            print(json.dumps({"case": case, **result, "parked_shutdown": True,
                              "port_file_removed": True, "listener_closed": True}), flush=True)
        finally:
            watchdog.cancel()
            if proc.poll() is None:
                proc.kill()
                proc.wait(timeout=5)
            reader.join(timeout=2)
            proc.stdout.close()
            print("".join(lines), end="", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path(
        os.environ.get("CARGO_TARGET_DIR", "target")) / "debug/bulwark-shell")
    parser.add_argument("--native", action="store_true")
    parser.add_argument("--timeout", type=float, default=35)
    args = parser.parse_args()
    if args.timeout <= 0:
        parser.error("--timeout must be positive")
    # Flags alone must not accidentally enable a listener or create a profile.
    with tempfile.TemporaryDirectory(prefix="bulwark-nrepl-disabled-") as directory:
        profile = Path(directory) / "unused"
        result = subprocess.run([str(args.binary), "--profile", str(profile)],
                                capture_output=True, timeout=args.timeout)
        if result.returncode == 0 or profile.exists():
            raise RuntimeError("Profile flag enabled nREPL without opt-in")
    for case in ["normal", "runtime-error", "dispatcher-error"] + (["close"] if args.native else []):
        run_case(args, case)


if __name__ == "__main__":
    main()
