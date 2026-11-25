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
    def run_markers(self, multiprocess, markers, expected_flag, backend=None):
        suffix = "-multiprocess" if multiprocess else ""
        if backend is None:
            backend = self.backend_markers()
        outputs = {expected_flag: markers, f"--document-scope-backend{suffix}": backend}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "probe"
            binary.write_text(
                f"#!{sys.executable}\nimport sys\n"
                f"print(chr(10).join({outputs!r}.get(sys.argv[1] if sys.argv[1:] else '', [])))\n"
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
    def markers(multiprocess=False):
        names = runner.EXPECTED_MARKERS - {"document-scope prototype"}
        if multiprocess:
            names |= runner.MULTIPROCESS_MARKERS
        return [f"S1 {name}: PASS" for name in sorted(names)] + [
            "S1 document-scope prototype: PASS"]

    @staticmethod
    def backend_markers():
        return [f"S1 {name}: PASS" for name in sorted(
            runner.BACKEND_MARKERS - {"backend-loss prototype"})] + [
            "S1 backend-loss prototype: PASS"]

    def test_single_process_mode(self):
        status, report = self.run_markers(False, self.markers(), "--document-scope")
        self.assertEqual(status, 0)
        self.assertFalse(report["multiprocess"])

    def test_multiprocess_mode(self):
        status, report = self.run_markers(
            True, self.markers(True), "--document-scope-multiprocess")
        self.assertEqual(status, 0)
        self.assertTrue(report["multiprocess"])
        self.assertEqual(report["cases"][1]["command"][1:], ["--document-scope-backend-multiprocess"])

    def test_backend_loss_requires_each_marker_and_summary(self):
        complete = self.backend_markers()
        for markers in (complete[:-1], complete[1:], complete[1:] + complete[:1], []):
            for multiprocess in (False, True):
                with self.subTest(markers=markers, multiprocess=multiprocess):
                    flag = "--document-scope-multiprocess" if multiprocess else "--document-scope"
                    status, report = self.run_markers(
                        multiprocess, self.markers(multiprocess), flag, backend=markers)
                    self.assertEqual(status, 1)
                    self.assertTrue(report["cases"][0]["assertions_complete"])
                    self.assertFalse(report["cases"][1]["assertions_complete"])

    def test_backend_markers_cannot_satisfy_scope_case(self):
        status, report = self.run_markers(False, self.backend_markers(), "--document-scope")
        self.assertEqual(status, 1)
        self.assertFalse(report["cases"][0]["assertions_complete"])
        self.assertTrue(report["cases"][1]["assertions_complete"])

    def test_multiprocess_requires_each_process_failure_mode(self):
        modes = sorted(runner.MULTIPROCESS_MARKERS)
        for missing in [set(modes), *({name} for name in modes)]:
            with self.subTest(missing=missing):
                markers = [line for line in self.markers(True)
                           if line.removeprefix("S1 ").split(": PASS")[0] not in missing]
                status, report = self.run_markers(True, markers, "--document-scope-multiprocess")
                self.assertEqual(status, 1)
                self.assertFalse(report["cases"][0]["assertions_complete"])

    def test_multiprocess_cannot_silently_run_single_process(self):
        status, _ = self.run_markers(True, self.markers(True), "--document-scope")
        self.assertEqual(status, 1)

    def test_duplicate_and_unexpected_markers_fail(self):
        for extra in ("S1 content-process-kill: PASS", "S1 unexpected: PASS"):
            with self.subTest(extra=extra):
                status, _ = self.run_markers(
                    True, self.markers(True) + [extra], "--document-scope-multiprocess")
                self.assertEqual(status, 1)

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
