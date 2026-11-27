"""Check S1 runner mode admission and cleanup without graphics services."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

import run_document_scope as runner


class DocumentScopeRunnerTests(unittest.TestCase):
    def run_markers(self, multiprocess, outputs=None, backend=None):
        """Run the runner against a fake binary. `outputs` maps argument strings to markers."""
        suffix = "-multiprocess" if multiprocess else ""
        if outputs is None:
            outputs = self.outputs(multiprocess)
        if backend is None:
            backend = self.backend_markers()
        outputs = {**outputs, f"--document-scope-backend{suffix}": backend}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "probe"
            binary.write_text(
                f"#!{sys.executable}\nimport sys\n"
                f"print(chr(10).join({outputs!r}.get(' '.join(sys.argv[1:]), [])))\n"
            )
            binary.chmod(0o755)
            probe = "experiments/e4-dom-channel/src/main.rs"
            (root / "provenance.json").write_text(json.dumps({
                "patch_sha256": runner.sha256(
                    runner.ROOT / "experiments/phase3/patches/servo-0.7-document-scope.patch"),
                "product_hashes": {name: runner.sha256(runner.ROOT / name)
                                   for name in ("Cargo.toml", "Cargo.lock")},
                "probe_hashes": {probe: runner.sha256(runner.ROOT / probe)},
                "candidate": str(runner.ROOT),
            }))
            command = [sys.executable, str(runner.ROOT / "experiments/run_document_scope.py"),
                       "--binary", str(binary), "--preparation", str(root),
                       "--output", str(root / "results"), "--repeat", "1"]
            if multiprocess:
                command.append("--multiprocess")
            result = subprocess.run(command, capture_output=True, text=True, timeout=10)
            report = json.loads((root / "results/results.json").read_text())
            return result.returncode, report

    @staticmethod
    def markers(part, names=None):
        names = runner.PARTS[part] if names is None else names
        return [f"S1 {name}: PASS" for name in sorted(names)] + [
            f"S1 document-scope {part}: PASS ({len(names)} cases; probe)"]

    @classmethod
    def outputs(cls, multiprocess, flag=None):
        flag = flag or ("--document-scope-multiprocess" if multiprocess else "--document-scope")
        return {f"{flag} {part}": cls.markers(part) for part in runner.parts(multiprocess)}

    @staticmethod
    def backend_markers():
        names = runner.BACKEND_MARKERS - {"backend-loss prototype"}
        return [f"S1 {name}: PASS" for name in sorted(names)] + [
            f"S1 backend-loss prototype: PASS ({len(names)} cases; probe)"]

    def case(self, report, name):
        return next(case for case in report["cases"] if case["name"] == name)

    def test_single_process_mode(self):
        status, report = self.run_markers(False)
        self.assertEqual(status, 0)
        self.assertFalse(report["multiprocess"])
        self.assertEqual([case["command"][1:] for case in report["cases"][:3]], [
            ["--document-scope", "documents"], ["--document-scope", "frames"],
            ["--document-scope-backend"]])
        self.assertEqual(report["repeats"], [{
            "repeat": 1, "cases": len(runner.SHARED_CASES), "assertions_complete": True}])

    def test_multiprocess_mode(self):
        status, report = self.run_markers(True)
        self.assertEqual(status, 0)
        self.assertTrue(report["multiprocess"])
        self.assertEqual([case["command"][1:] for case in report["cases"][:4]], [
            ["--document-scope-multiprocess", "documents"],
            ["--document-scope-multiprocess", "frames"],
            ["--document-scope-multiprocess", "processes"],
            ["--document-scope-backend-multiprocess"]])
        self.assertEqual(report["repeats"], [{
            "repeat": 1, "cases": len(runner.MULTIPROCESS_CASES), "assertions_complete": True}])

    def test_backend_loss_requires_each_marker_and_summary(self):
        complete = self.backend_markers()
        for markers in (complete[:-1], complete[1:], complete[1:] + complete[:1], []):
            for multiprocess in (False, True):
                with self.subTest(markers=markers, multiprocess=multiprocess):
                    status, report = self.run_markers(multiprocess, backend=markers)
                    self.assertEqual(status, 1)
                    self.assertTrue(self.case(report, "document-scope-documents-1")[
                        "assertions_complete"])
                    self.assertFalse(self.case(report, "backend-loss-1")["assertions_complete"])

    def test_backend_markers_cannot_satisfy_scope_case(self):
        outputs = self.outputs(False)
        outputs["--document-scope frames"] = self.backend_markers()
        status, report = self.run_markers(False, outputs)
        self.assertEqual(status, 1)
        self.assertFalse(self.case(report, "document-scope-frames-1")["assertions_complete"])
        self.assertTrue(self.case(report, "backend-loss-1")["assertions_complete"])
        self.assertFalse(report["repeats"][0]["assertions_complete"])

    def test_each_part_requires_each_case(self):
        # Every frame and process case, and the first, last, and parse cases of the documents part.
        documents = sorted(runner.DOCUMENT_CASES)
        checked = {"documents": {documents[0], documents[-1], "cross-site-parser-blocked-failure"},
                   "frames": runner.FRAME_CASES, "processes": runner.PROCESS_CASES}
        for multiprocess in (False, True):
            for part in runner.parts(multiprocess):
                for missing in sorted(checked[part]):
                    with self.subTest(multiprocess=multiprocess, part=part, missing=missing):
                        outputs = self.outputs(multiprocess)
                        flag = next(key for key in outputs if key.endswith(f" {part}"))
                        outputs[flag] = [line for line in outputs[flag]
                                         if line.removeprefix("S1 ").split(": PASS")[0] != missing]
                        status, report = self.run_markers(multiprocess, outputs)
                        self.assertEqual(status, 1)
                        self.assertFalse(self.case(report, f"document-scope-{part}-1")[
                            "assertions_complete"])

    def test_each_part_requires_its_own_process(self):
        # A part cannot pass by printing another part's markers, even when the total is right.
        for multiprocess in (False, True):
            with self.subTest(multiprocess=multiprocess):
                flag = "--document-scope-multiprocess" if multiprocess else "--document-scope"
                frames = sorted(runner.FRAME_CASES)
                outputs = self.outputs(multiprocess)
                outputs[f"{flag} documents"] = self.markers(
                    "documents", runner.DOCUMENT_CASES | {frames[0]})
                outputs[f"{flag} frames"] = self.markers("frames", set(frames[1:]))
                status, report = self.run_markers(multiprocess, outputs)
                self.assertEqual(status, 1)
                self.assertFalse(self.case(report, "document-scope-documents-1")[
                    "assertions_complete"])
                self.assertFalse(self.case(report, "document-scope-frames-1")[
                    "assertions_complete"])
                self.assertFalse(report["repeats"][0]["assertions_complete"])

    def test_processes_part_runs_only_in_multiprocess_mode(self):
        self.assertEqual(runner.parts(False), ["documents", "frames"])
        self.assertEqual(runner.parts(True), ["documents", "frames", "processes"])
        status, report = self.run_markers(False)
        self.assertEqual(status, 0)
        self.assertFalse(any("processes" in case["name"] for case in report["cases"]))

    def test_multiprocess_cannot_silently_run_single_process(self):
        status, _ = self.run_markers(True, self.outputs(True, "--document-scope"))
        self.assertEqual(status, 1)

    def test_duplicate_and_unexpected_markers_fail(self):
        for extra in ("S1 content-process-kill: PASS", "S1 unexpected: PASS"):
            with self.subTest(extra=extra):
                outputs = self.outputs(True)
                outputs["--document-scope-multiprocess processes"].insert(0, extra)
                status, _ = self.run_markers(True, outputs)
                self.assertEqual(status, 1)

    def test_stale_summary_count_fails(self):
        for multiprocess in (False, True):
            for part in runner.parts(multiprocess):
                with self.subTest(multiprocess=multiprocess, part=part):
                    outputs = self.outputs(multiprocess)
                    flag = next(key for key in outputs if key.endswith(f" {part}"))
                    count = len(outputs[flag]) - 1
                    outputs[flag][-1] = outputs[flag][-1].replace(
                        f"({count} cases;", f"({count - 1} cases;")
                    status, report = self.run_markers(multiprocess, outputs)
                    self.assertEqual(status, 1)
                    self.assertFalse(self.case(report, f"document-scope-{part}-1")[
                        "assertions_complete"])

    def test_documented_case_counts(self):
        # The scope guide and handoff state these counts. Keep them in step with the markers.
        self.assertEqual(len(runner.DOCUMENT_CASES), 51)
        self.assertEqual(len(runner.FRAME_CASES), 8)
        self.assertEqual(len(runner.PROCESS_CASES), 28)
        self.assertEqual(len(runner.SHARED_CASES), 59)
        self.assertEqual(len(runner.MULTIPROCESS_CASES), 87)

    def test_timeout_stops_content_processes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            escaped = root / "escaped"
            child = f"import time; from pathlib import Path; time.sleep(3); Path({str(escaped)!r}).touch()"
            parent = (
                "import subprocess, sys, time; "
                f"subprocess.Popen([sys.executable, '-c', {child!r}]); "
                "print('child started', flush=True); time.sleep(10)"
            )
            with (root / "output.log").open("w") as log:
                status = runner.run_case([sys.executable, "-c", parent], log, timeout=1.5)
            self.assertEqual(status, "timeout")
            self.assertIn("child started", (root / "output.log").read_text())
            # The child would touch the file 3 seconds after the parent started.
            time.sleep(2)
            self.assertFalse(escaped.exists(), "The content process survived the deadline")


if __name__ == "__main__":
    unittest.main()
