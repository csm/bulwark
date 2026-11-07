"""Assert runtime-thread nREPL concurrency, browser calls, and shutdown."""
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

from nrepl_probe import decode, encode


NS = "bulwark.experiment.nrepl"


class Client:
    def __init__(self, stack, port):
        self.socket = stack.enter_context(socket.create_connection(("127.0.0.1", port), timeout=5))
        self.stream = stack.enter_context(self.socket.makefile("rb"))
        self.sequence = 0
        self.session = None
        replies = self.request(op="clone")
        self.session = next(reply["new-session"] for reply in replies if "new-session" in reply)

    def send(self, request_id, **fields):
        self.socket.sendall(encode({"id": request_id,
                                   **({"session": self.session} if self.session else {}), **fields}))

    def collect(self, request_id, interrupted=False, overloaded=False):
        replies = []
        while True:
            reply = decode(self.stream)
            if reply.get("id") != request_id:
                raise RuntimeError(f"Out-of-order response: {reply}, expected {request_id}")
            if not overloaded and (reply.get("err") or "eval-error" in reply.get("status", [])):
                raise RuntimeError(reply)
            replies.append(reply)
            if "done" in reply.get("status", []):
                if interrupted and not any("interrupted" in r.get("status", []) for r in replies):
                    raise RuntimeError(f"Evaluation was not interrupted: {replies}")
                if overloaded and not any("overloaded" in r.get("status", []) for r in replies):
                    raise RuntimeError(f"Request was not refused under overload: {replies}")
                return replies

    def request(self, **fields):
        self.sequence += 1
        request_id = f"request-{self.sequence}"
        self.send(request_id, **fields)
        return self.collect(request_id)

    def evaluate(self, code):
        replies = self.request(op="eval", code=code)
        values = [reply["value"] for reply in replies if "value" in reply]
        if not values:
            raise RuntimeError(f"Evaluation returned no value: {replies}")
        return values[-1]

    def expect(self, code, expected):
        value = self.evaluate(code)
        if value != expected:
            raise RuntimeError(f"{code}: expected {expected!r}, got {value!r}")

    def started(self, request_id, marker):
        while True:
            reply = decode(self.stream)
            if reply.get("id") != request_id or reply.get("err") or "status" in reply:
                raise RuntimeError(f"Evaluation did not reach its marker: {reply}")
            if marker in reply.get("out", ""):
                return


def interrupt(controller, evaluator, request_id):
    started = time.monotonic()
    controller.request(op="interrupt", session=evaluator.session, **{"interrupt-id": request_id})
    evaluator.collect(request_id, interrupted=True)
    return (time.monotonic() - started) * 1000


def parked_eval(client, request_id):
    client.send(request_id, op="eval", code='(do (println "E7-parked") (Thread/sleep 60000) :late)')
    client.started(request_id, "E7-parked")


def overload_probe(a, b, controller):
    # Parked and queued jobs must retain their slots, even after host admission.
    parked_eval(a, "overload-a")
    parked_eval(b, "overload-b")
    for index in range(23):
        a.send(f"queued-a-{index}", op="eval", code=f"(swap! {NS}/order conj -1)")
    a.send("session-overflow", op="eval", code="42")
    a.collect("session-overflow", overloaded=True)
    # A different session still reaches the engine with the first session full.
    controller.expect(f'({NS}/eval-js! "\\\"overload-peer\\\"")', '"overload-peer"')
    for index in range(7):
        b.send(f"queued-b-{index}", op="eval", code=f"(swap! {NS}/order conj -1)")
    # This response is a network-order barrier after b's seven submissions.
    b.request(op="describe")
    controller.send("global-overflow", op="eval", code="42")
    controller.collect("global-overflow", overloaded=True)
    controller.request(op="describe")
    controller.request(op="interrupt", session=a.session)
    a.collect("overload-a", interrupted=True)
    for index in range(23):
        a.collect(f"queued-a-{index}", interrupted=True)
    controller.request(op="interrupt", session=b.session)
    b.collect("overload-b", interrupted=True)
    for index in range(7):
        b.collect(f"queued-b-{index}", interrupted=True)
    a.expect("(+ 20 22)", "42")
    b.expect("(+ 20 22)", "42")
    controller.expect(f"@{NS}/order", "[]")
    return {"session_overload_refused": True, "global_overload_refused": True,
            "interrupt_under_overload": True, "capacity_recovered": True}


def probe(a, b, controller, real_servo):
    overload = overload_probe(a, b, controller)
    # One session retains its queue while another session serves browser commands.
    parked_eval(a, "parked")
    a.send("after-parked", op="eval", code="(+ 20 22)")
    before = int(b.evaluate(f"@{NS}/ticks"))
    b.expect(f'({NS}/eval-js! "\\\"parked-peer\\\"")', '"parked-peer"')
    b.evaluate("(Thread/sleep 30)")
    if int(b.evaluate(f"@{NS}/ticks")) <= before:
        raise RuntimeError("Browser-program heartbeat stopped behind the parked session")
    parked_ms = interrupt(b, a, "parked")
    if not any(reply.get("value") == "42" for reply in a.collect("after-parked")):
        raise RuntimeError("Queued work in the interrupted session did not recover")

    # The interpreter's automatic fiber yields must preserve progress during CPU work too.
    b.evaluate(f'({NS}/engine-marker! "stall-begin")')
    before = int(b.evaluate(f"@{NS}/ticks"))
    a.send("busy", op="eval", code='(do (println "E7-running") (loop [n 0] (recur (inc n))))')
    a.started("busy", "E7-running")
    b.expect(f'({NS}/eval-js! "\\\"cpu-peer\\\"")', '"cpu-peer"')
    b.evaluate("(Thread/sleep 80)")
    if int(b.evaluate(f"@{NS}/ticks")) <= before:
        raise RuntimeError("CPU-bound evaluation starved the browser-program heartbeat")
    if real_servo:
        ticks = json.loads(b.evaluate(f'({NS}/eval-js! "String(window.__e7ticks)")'))
        if int(ticks) < 5:
            raise RuntimeError("Servo page timer did not progress")
    busy_ms = interrupt(b, a, "busy")
    a.expect("(+ 20 22)", "42")
    turns = int(b.evaluate(f'({NS}/engine-marker! "stall-end")'))
    if turns <= 0:
        raise RuntimeError("Engine did not advance during the busy evaluation")

    # An accepted burst retains session order while other sessions make progress.
    count = 20
    a.socket.sendall(b"".join(encode({
        "id": f"burst-{index}", "op": "eval", "session": a.session,
        "code": f"(do (Thread/sleep 20) (swap! {NS}/order conj {index}) {index})",
    }) for index in range(count)))
    before = int(b.evaluate(f"@{NS}/ticks"))
    b.evaluate("(Thread/sleep 50)")
    b.expect(f'({NS}/eval-js! "\\\"burst-peer\\\"")', '"burst-peer"')
    if int(b.evaluate(f"@{NS}/ticks")) <= before:
        raise RuntimeError("Queued evaluations starved the browser program")
    for index in range(count):
        replies = a.collect(f"burst-{index}")
        if not any(reply.get("value") == str(index) for reply in replies):
            raise RuntimeError(f"Queued evaluation {index} returned the wrong value")
    b.expect(f"@{NS}/order", "[" + " ".join(map(str, range(count))) + "]")

    b.evaluate('(defn probe-label [] "before")')
    a.expect("(probe-label)", '"before"')
    b.evaluate('(defn probe-label [] "after")')
    a.expect("(probe-label)", '"after"')
    return {**overload, "parked_interrupt_ms": parked_ms, "busy_interrupt_ms": busy_ms,
            "engine_turns_during_busy_eval": turns, "ordered_burst": count,
            "cross_session_browser_calls": True, "live_redefinition": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default=str(
        Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "debug/bulwark-experiment-e7"))
    parser.add_argument("--servo", action="store_true")
    parser.add_argument("--native", action="store_true")
    parser.add_argument("--close-window", action="store_true")
    parser.add_argument("--timeout", type=float, default=35)
    args = parser.parse_args()
    if args.timeout <= 0:
        parser.error("--timeout must be positive")
    if args.close_window and not args.native:
        parser.error("--close-window requires --native")

    with tempfile.TemporaryDirectory(prefix="bulwark-e7-nrepl-") as directory:
        port_file = Path(directory) / "nrepl-port"
        command = [args.binary, "--nrepl", "--nrepl-port-file", str(port_file)]
        if args.native:
            command.append("--native")
        elif args.servo:
            command.append("--servo")
        if args.close_window:
            command.extend(["--close-after-ms", "1500"])
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
                    raise RuntimeError("Child exited before reporting nREPL")
                match = re.fullmatch(r"nREPL: 127\.0\.0\.1:([1-9]\d*)\s*", line)
                if match:
                    port = int(match[1])
            if port_file.read_text().strip() != str(port):
                raise RuntimeError("nREPL port file does not match the listener")
            with ExitStack() as stack:
                a, b, controller = Client(stack, port), Client(stack, port), Client(stack, port)
                while b.evaluate(f"@{NS}/ready") != "true":
                    time.sleep(0.01)
                result = {} if args.close_window else probe(a, b, controller, args.native or args.servo)
                parked_eval(a, "shutdown-parked")
                if not args.close_window:
                    # The server can close before this final eval's reply drains.
                    b.send("stop", op="eval", code=f"(reset! {NS}/running false)")
                proc.wait(timeout=args.timeout)
                reader.join(timeout=2)
                if expired.is_set() or proc.returncode != 0:
                    raise RuntimeError(f"Child failed or exceeded its process deadline: {proc.returncode}")
                try:
                    while True:
                        decode(a.stream)
                except (EOFError, ConnectionError):
                    pass
                if port_file.exists():
                    raise RuntimeError("nREPL port file survived shutdown")
                try:
                    connection = socket.create_connection(("127.0.0.1", port), timeout=1)
                except ConnectionError:
                    pass
                else:
                    connection.close()
                    raise RuntimeError("nREPL listener survived shutdown")
            output = "".join(lines)
            if "E7 nREPL: admission_calls=" not in output:
                raise RuntimeError("Missing nREPL admission measurements")
            limits = re.search(r"max_jobs=(\d+), max_pending=(\d+), budget_exhausted=(\d+)", output)
            if not limits or int(limits[1]) > 4 or int(limits[2]) > 32:
                raise RuntimeError("nREPL exceeded its admission limits or omitted measurements")
            if args.native:
                if "shutdown=joined" not in output:
                    raise RuntimeError("Native host did not join the runtime")
                if not args.close_window:
                    frames = re.search(r"presented_frames_during_stall=(\d+)", output)
                    if not frames or int(frames[1]) <= 0:
                        raise RuntimeError("Native rendering stopped during nREPL CPU work")
            elif args.servo:
                frames = re.search(r"painted_frames_during_stall=(\d+)", output)
                if not frames or int(frames[1]) <= 0:
                    raise RuntimeError("Servo did not paint during nREPL CPU work")
            print(json.dumps({**result, "shutdown_with_parked_eval": True,
                              "port_file_removed": True, "listener_closed": True}), flush=True)
        finally:
            watchdog.cancel()
            if proc.poll() is None:
                proc.kill()
                proc.wait(timeout=5)
            reader.join(timeout=2)
            print("".join(lines), end="", flush=True)


if __name__ == "__main__":
    main()
