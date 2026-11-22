"""Compare release probes on the same Wikipedia fundraising banner preview."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
from urllib.parse import urlencode

from compare_intersection_observer import probe


OBSERVE = """(() => {
  if (window !== window.top) return;
  setInterval(() => {
    const cn = globalThis.mw?.centralNotice;
    const banner = cn?.bannerData?.bannerName ?? cn?.getDataProperty?.('banner');
    console.warn('radio-banner-observation ' + JSON.stringify({
      banner, radios: document.querySelectorAll('input[type=radio]').length,
      populated: !!document.querySelector('#centralNotice')?.children.length
    }));
  }, 12000);
})();"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--banner", required=True)
    parser.add_argument("--repeat", type=int, default=1)
    parser.add_argument("--seconds", type=int, default=45)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"[A-Za-z0-9_-]+", args.banner):
        parser.error("Use the observed CentralNotice banner identifier")
    if args.repeat < 1 or not 21 <= args.seconds <= 300:
        parser.error("--repeat must be positive; --seconds must be 21 to 300")
    binaries = {label: path.resolve(strict=True) for label, path in
                [("baseline", args.baseline), ("candidate", args.candidate)]}
    if binaries["baseline"].samefile(binaries["candidate"]):
        parser.error("Baseline and candidate must be separate executables")
    args.out.mkdir(parents=True, exist_ok=False)
    script = (args.out / "observe-banner.js").resolve()
    script.write_text(OBSERVE)
    url = "https://en.wikipedia.org/wiki/Rust_%28programming_language%29?" + urlencode(
        {"banner": args.banner, "force": "1", "uselang": "en"})
    report = {"started_utc": datetime.now(timezone.utc).isoformat(), "url": url,
              "expected_banner": args.banner, "seconds": args.seconds,
              "binaries": {label: {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
                           for label, path in binaries.items()}, "cases": [], "completed": False}
    for iteration in range(args.repeat):
        order = list(binaries) if iteration % 2 == 0 else list(reversed(binaries))
        for label in order:
            name = f"{iteration + 1:02d}-{label}"
            command = [str(binaries[label]), url, "--intersection-observer", "--adopted-stylesheets",
                       "--measure", "--seconds", str(args.seconds), "--user-script", str(script)]
            result = probe(command, args.out, name, args.seconds, False, 3)
            log = result.pop("log")
            observations = [json.loads(line.removeprefix("console Warn: radio-banner-observation "))
                            for line in log.splitlines()
                            if line.startswith("console Warn: radio-banner-observation ")]
            delays = [int(n) for n in re.findall(r"heartbeat: ms=(\d+) ok=true", log)]
            turns = re.findall(r"longest-turn-ms: (\d+)", log)
            case = {"name": name, "command": command, **result, "observations": observations,
                    "heartbeats": len(delays), "max_heartbeat_ms": max(delays, default=None),
                    "longest_turn_ms": int(turns[-1]) if turns else None,
                    "inspections": sum(line.startswith("inspection: ") for line in log.splitlines()),
                    "banner_observed": any(o.get("banner") == args.banner and o.get("populated")
                                           for o in observations)}
            report["cases"].append(case)
            (args.out / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
            print(json.dumps(case), flush=True)
            if case["exit_status"] != 0 or case["timed_out"] or not case["banner_observed"] or not delays:
                raise RuntimeError(f"{name}: incomplete matching-banner comparison")
    report["completed"] = True
    (args.out / "summary.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
