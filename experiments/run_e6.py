"""Check mid-form interruption, recovery, and queued work on the E1 host."""
import argparse
from contextlib import ExitStack
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import time

from nrepl_probe import decode, encode


def collect(stream, request_id):
    replies = []
    while True:
        reply = decode(stream)
        if reply.get("id") != request_id:
            raise RuntimeError(f"Unexpected reply: {reply}")
        if reply.get("err") or "eval-error" in reply.get("status", []):
            raise RuntimeError(reply)
        replies.append(reply)
        if "done" in reply.get("status", []):
            return replies


def request(client, stream, request_id, **fields):
    client.sendall(encode({"id": request_id, **fields}))
    return collect(stream, request_id)


def probe(port):
    with ExitStack() as stack:
        evaluator = stack.enter_context(socket.create_connection(("127.0.0.1", port), timeout=3))
        interrupter = stack.enter_context(socket.create_connection(("127.0.0.1", port), timeout=3))
        eval_stream = stack.enter_context(evaluator.makefile("rb"))
        interrupt_stream = stack.enter_context(interrupter.makefile("rb"))
        cloned = request(evaluator, eval_stream, "clone", op="clone")
        session = next(r["new-session"] for r in cloned if "new-session" in r)
        evaluator.sendall(encode({
            "op": "eval", "id": "busy", "session": session,
            "code": '(do (println "E6-running") (loop [n 0] (recur (inc n))))',
        }))
        # Streaming output proves evaluation has begun; queued cancellation is insufficient.
        while True:
            reply = decode(eval_stream)
            if "E6-running" in reply.get("out", ""):
                break
            if reply.get("err") or "status" in reply:
                raise RuntimeError(f"Busy form did not start: {reply}")
        time.sleep(0.2)
        started = time.monotonic()
        acknowledgement = request(interrupter, interrupt_stream, "stop", op="interrupt",
                                  session=session, **{"interrupt-id": "busy"})
        if acknowledgement[-1].get("status") != ["done"]:
            raise RuntimeError(f"Interrupt was not accepted: {acknowledgement}")
        interrupted = collect(eval_stream, "busy")
        latency_ms = (time.monotonic() - started) * 1000
        if not any("interrupted" in r.get("status", []) for r in interrupted):
            raise RuntimeError(f"Running form was not interrupted: {interrupted}")
        recovered = request(evaluator, eval_stream, "recover", op="eval", session=session,
                            code="(do (swap! bulwark.experiment/nrepl-evals inc) (+ 20 22))")
        if not any(r.get("value") == "42" for r in recovered):
            raise RuntimeError(f"Session did not recover: {recovered}")
        # A go block only yields at an await. A blocking native call inside it
        # must appear in max_slice rather than being mistaken for a bounded slice.
        started = time.monotonic()
        request(evaluator, eval_stream, "go-stall", op="eval", session=session,
                code="(clojure.core.async/go (Thread/sleep 200) "
                     "(swap! bulwark.experiment/nrepl-evals inc))")
        for i in range(100):
            finished = request(evaluator, eval_stream, f"go-finished-{i}", op="eval",
                               session=session, code="@bulwark.experiment/nrepl-evals")
            if any(r.get("value") == "2" for r in finished):
                break
            time.sleep(0.01)
        else:
            raise RuntimeError("Blocking go task did not recover")
        go_ms = (time.monotonic() - started) * 1000
        before = request(evaluator, eval_stream, "before", op="eval", session=session,
                         code="@bulwark.experiment/async-ticks")
        # Queue 20 small blocking calls together. The host's max_poll reports whether
        # a zero-timeout poll bounds the combined work to a frame budget.
        started = time.monotonic()
        evaluator.sendall(b"".join(encode({
            "op": "eval", "id": f"burst-{i}", "session": session,
            "code": "(Thread/sleep 10)",
        }) for i in range(20)))
        for i in range(20):
            collect(eval_stream, f"burst-{i}")
        burst_ms = (time.monotonic() - started) * 1000
        time.sleep(0.25)
        after = request(evaluator, eval_stream, "after", op="eval", session=session,
                        code="@bulwark.experiment/async-ticks")
        before_ticks = int(next(r["value"] for r in before if "value" in r))
        after_ticks = int(next(r["value"] for r in after if "value" in r))
        if after_ticks <= before_ticks:
            raise RuntimeError("Async loop did not recover")
        return {"interrupt_latency_ms": latency_ms, "burst_round_trip_ms": burst_ms,
                "blocking_go_round_trip_ms": go_ms,
                "async_ticks_before": before_ticks, "async_ticks_after": after_ticks,
                "session_recovered": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native", action="store_true")
    parser.add_argument("--worker", action="store_true")
    parser.add_argument("--binary", default=str(Path(os.environ.get("CARGO_TARGET_DIR", "target")) /
                                               "debug/bulwark-experiment-e1"))
    args = parser.parse_args()
    if args.worker and not args.native:
        parser.error("--worker requires --native")
    proc = subprocess.Popen(
        [args.binary, *([] if args.native else ["--headless"]),
         *(["--worker"] if args.worker else []), "--seconds", "8"],
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
    )
    try:
        first = proc.stdout.readline()
        print(first, end="", flush=True)
        match = re.fullmatch(r"nREPL: 127\.0\.0\.1:([1-9]\d*)\s*", first)
        if not match:
            raise RuntimeError("E1 did not report an nREPL listener")
        print(json.dumps(probe(int(match[1]))), flush=True)
        output, _ = proc.communicate(timeout=12)
        print(output, end="", flush=True)
        if proc.returncode:
            raise SystemExit(proc.returncode)
    finally:
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()


if __name__ == "__main__":
    main()
