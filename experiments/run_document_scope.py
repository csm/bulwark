"""Run the isolated S1 regressions and existing E4 controls with retained evidence."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import time


ROOT = Path(__file__).resolve().parent.parent
# Each part runs in its own engine process and prints its own summary marker.
DOCUMENT_CASES = {
    "initial-blank-and-tagged-result", "lifecycle-readiness-and-current-delivery",
    "same-document-navigation", "held-A-after-B-and-unscoped-control",
    "queued-reset-race-and-pipeline-collision", "serialization-reset",
    "delayed-host-delivery", "host-cancellation", "reload", "history", "redirect",
    "failed-load", "cross-view", "subframe-isolation", "close", "injected-crash",
    "script-created-initial-document", "javascript-url-replacement",
    "script-thread-failure-and-callback-cleanup", "unconstructed-view-failure",
    "pending-navigation-failure", "crash-recovery-history", "crash-history-reload",
    "headers-before-parse-failure", "parser-blocked-failure", "about-blank-failure",
    "about-blank-write-failure", "srcdoc-parser-blocked-failure",
    "document-write-parser-blocked-failure", "document-open-write-failure",
    "javascript-url-crash-recovery", "cross-site-parser-blocked-failure",
    "final-handle-release", "release-before-document-activation", "engine-shutdown-cleanup",
    "result-callback-panic", "lifecycle-callback-panics", "release-callback-panics",
    "queued-engine-cancellation", "completed-engine-cancellation",
    "engine-cancellation-callback-panic", "active-engine-interruption",
    "serialization-interruption", "interrupted-scheduled-work", "uninterrupted-long-evaluation",
    "release-during-host-unwind", "shutdown-callback-panics", "lifecycle-delegate-panic",
    "release-delegate-panic", "delegate-panic-during-host-unwind", "shutdown-delegate-panics",
}
FRAME_CASES = {
    "cross-site-subframe-parser-blocked-failure", "sandboxed-srcdoc-parser-blocked-failure",
    "same-site-frame-pending-navigation-failure", "cross-site-frame-pending-navigation-failure",
    "cross-site-lone-frame-pending-navigation-failure", "frame-initial-navigation-failure",
    "frame-healthy-pending-navigation-failure", "frame-history-failure",
}
PROCESS_CASES = {
    "content-process-failure", "content-process-kill", "shared-process-close",
    "shared-process-failure", "shared-process-kill",
    "shared-process-close-kill-race", "unconstructed-process-exit", "unconstructed-process-kill",
    "pending-navigation-process-exit", "pending-navigation-process-kill",
    "cross-site-parser-blocked-process-exit", "cross-site-parser-blocked-process-kill",
    "cross-site-subframe-process-exit", "cross-site-subframe-process-kill",
    "sandboxed-srcdoc-process-exit", "sandboxed-srcdoc-process-kill",
    "same-site-frame-pending-process-exit", "same-site-frame-pending-process-kill",
    "cross-site-frame-pending-process-exit", "cross-site-frame-pending-process-kill",
    "cross-site-lone-frame-pending-process-exit", "cross-site-lone-frame-pending-process-kill",
    "frame-initial-navigation-process-exit", "frame-initial-navigation-process-kill",
    "frame-healthy-pending-process-exit", "frame-healthy-pending-process-kill",
    "frame-history-process-exit", "frame-history-process-kill",
}
# The processes part ends content processes, so it runs only in multiprocess mode.
PARTS = {"documents": DOCUMENT_CASES, "frames": FRAME_CASES, "processes": PROCESS_CASES}
MULTIPROCESS_PARTS = {"processes"}
SHARED_CASES = DOCUMENT_CASES | FRAME_CASES
MULTIPROCESS_CASES = SHARED_CASES | PROCESS_CASES
assert not (DOCUMENT_CASES & FRAME_CASES or SHARED_CASES & PROCESS_CASES), "A case is in two parts"
# Backend loss stops the engine, so it runs in its own process after each scope case.
BACKEND_MARKERS = {"backend-disconnection", "drop-after-backend-loss", "backend-loss prototype"}


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def assertions_complete(markers, expected, summary):
    return (len(markers) == len(expected)
            and {line.removeprefix("S1 ").split(": PASS", 1)[0] for line in markers} == expected
            and all(": PASS" in line for line in markers)
            # The summary's own case count must match, so a stale count cannot pass.
            and markers[-1].startswith(f"S1 {summary}: PASS ({len(expected) - 1} cases;"))


def parts(multiprocess):
    return [part for part in PARTS if multiprocess or part not in MULTIPROCESS_PARTS]


def repeat_complete(part_markers, multiprocess):
    """Check one repeat across its parts. Each part's own markers must already be exact."""
    names = [line.removeprefix("S1 ").split(": PASS", 1)[0]
             for part in parts(multiprocess) for line in part_markers.get(part, [])[:-1]]
    expected = MULTIPROCESS_CASES if multiprocess else SHARED_CASES
    return len(names) == len(expected) and set(names) == expected


def run_case(command, log, timeout):
    # Servo content processes inherit this group. A deadline must stop them too.
    with subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT,
                          start_new_session=True) as process:
        try:
            return process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            return "timeout"
        finally:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--preparation", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--multiprocess", action="store_true",
                        help="Run every S1 part with IPC, including the processes part")
    args = parser.parse_args()
    if not 1 <= args.repeat <= 20:
        parser.error("--repeat must be between 1 and 20")
    binary = args.binary.resolve(strict=True)
    provenance = json.loads((args.preparation / "provenance.json").read_text())
    patch = ROOT / "experiments/phase3/patches/servo-0.7-document-scope.patch"
    if provenance["patch_sha256"] != sha256(patch):
        parser.error("The preparation does not match the current patch")
    for name, expected in provenance["product_hashes"].items():
        if sha256(ROOT / name) != expected:
            parser.error(f"The preparation does not match the product manifest: {name}")
    if not provenance.get("probe_hashes"):
        parser.error("Recreate the preparation to record candidate probe hashes")
    for name, expected in provenance["probe_hashes"].items():
        candidate_path = Path(provenance["candidate"]) / name
        if sha256(ROOT / name) != expected or sha256(candidate_path) != expected:
            parser.error(f"The preparation does not match the current probe: {name}")
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
        "multiprocess": args.multiprocess,
        "scope": ("Experimental engine scope, host allocation gate, script-thread unwind, "
                  "final-handle release, explicit engine shutdown, queued execution cancellation, "
                  "interruption of running JavaScript, content-process exit and forced kill in multiprocess mode, "
                  "shared-process view revocation and ordinary sibling close, "
                  "script-thread failure and content-process exit or kill before document construction, "
                  "crash recovery through reload and history, "
                  "script-thread failure after response headers and in about:blank, srcdoc, "
                  "document.write, and document.open() construction, "
                  "crash recovery of a javascript: URL document to an error page without its result, "
                  "script-thread failure while a cross-site navigation, a cross-site subframe, "
                  "or a sandboxed srcdoc child parses in its own event loop, "
                  "content-process exit or kill while a cross-site document, a cross-site subframe, "
                  "or a sandboxed srcdoc child parses, "
                  "script-thread failure and content-process exit or kill while a same-site or "
                  "cross-site child frame's navigation waits for its response headers, "
                  "containment of a child frame's failure to that frame when the parent's own "
                  "top-level document runs in another event loop, including a frame's first "
                  "navigation, a failed frame with a healthy pending navigation, and frame "
                  "history after a contained failure, "
                  "callback and delegate unwind safety, and constellation loss in a separate engine. "
                  "Remaining replacement audits remain open."),
        "cases": [],
    }
    suffix = "-multiprocess" if args.multiprocess else ""
    # Each S1 part must print exactly its own markers, ending with its summary marker.
    s1_modes = {
        (f"--document-scope{suffix}", part): (
            PARTS[part] | {f"document-scope {part}"}, f"document-scope {part}")
        for part in parts(args.multiprocess)
    }
    s1_modes[(f"--document-scope-backend{suffix}",)] = (BACKEND_MARKERS, "backend-loss prototype")
    cases = []
    for i in range(args.repeat):
        cases += [(f"document-scope-{part}-{i + 1}", [f"--document-scope{suffix}", part], i)
                  for part in parts(args.multiprocess)]
        cases.append((f"backend-loss-{i + 1}", [f"--document-scope-backend{suffix}"], None))
    cases += [("servo-contracts", ["--phase3"], None), ("e4-baseline", [], None)]
    report["parts"] = parts(args.multiprocess)
    report["repeats"] = []
    repeat_markers = {}
    for name, flags, repeat in cases:
        started = time.monotonic()
        command = [str(binary), *flags]
        log_path = args.output / f"{name}.log"
        mode = s1_modes.get(tuple(flags))
        with log_path.open("w") as log:
            status = run_case(command, log, timeout=(
                90 if args.multiprocess and mode else 35))
        log = log_path.read_text(errors="replace")
        markers = [line for line in log.splitlines() if line.startswith("S1 ")]
        # An unpatched E4 binary must not pass by silently running its baseline path.
        complete = mode is None or assertions_complete(markers, *mode)
        report["cases"].append({
            "name": name, "command": command, "status": status,
            "elapsed_seconds": time.monotonic() - started,
            "assertions_complete": complete, "markers": markers,
            "log_sha256": sha256(log_path),
        })
        if repeat is not None:
            repeat_markers.setdefault(repeat, {})[flags[1]] = markers if complete else []
            if len(repeat_markers[repeat]) == len(report["parts"]):
                # All parts of one repeat together must cover every case of this mode once.
                report["repeats"].append({
                    "repeat": repeat + 1,
                    "cases": sum(len(found) - 1 for found in repeat_markers[repeat].values()
                                 if found),
                    "assertions_complete": repeat_complete(
                        repeat_markers[repeat], args.multiprocess),
                })
        (args.output / "results.json").write_text(json.dumps(report, indent=2) + "\n")
        print(f"{name}: {status}, assertions_complete={complete}", flush=True)
    if (any(case["status"] != 0 or not case["assertions_complete"] for case in report["cases"])
            or len(report["repeats"]) != args.repeat
            or not all(repeat["assertions_complete"] for repeat in report["repeats"])):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
