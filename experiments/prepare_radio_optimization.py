"""Prepare isolated baseline/candidate workspaces for the Servo radio experiment."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess


ROOT = Path(__file__).resolve().parent.parent
PATCH = ROOT / "experiments/e3-servo/patches/servo-0.7-radio-unrooted.patch"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--servo-script-source", type=Path)
    args = parser.parse_args()
    source = args.servo_script_source
    if source is None:
        cargo_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
        found = list((cargo_home / "registry/src").glob("*/servo-script-0.7.0"))
        if len(found) != 1:
            parser.error("Specify --servo-script-source for the published 0.7.0 package")
        source = found[0]
    source = source.resolve(strict=True)
    manifest = (source / "Cargo.toml").read_text()
    if 'name = "servo-script"' not in manifest or 'version = "0.7.0"' not in manifest:
        parser.error("The source must be the published servo-script 0.7.0 package")
    output = args.out.resolve()
    # Avoid copying this output tree back into itself during workspace preparation.
    for tree in [ROOT / "crates", ROOT / "clj", ROOT / "experiments"]:
        if output == tree or tree in output.parents:
            parser.error("Place --out outside the source trees, for example under target/")
    output.mkdir(parents=True, exist_ok=False)
    baseline = output / "baseline"
    baseline.mkdir()
    for name in ["Cargo.toml", "Cargo.lock"]:
        shutil.copy2(ROOT / name, baseline / name)
    for name in ["crates", "clj", "experiments"]:
        shutil.copytree(ROOT / name, baseline / name,
                        ignore=shutil.ignore_patterns("target", "__pycache__"))
    candidate = output / "candidate"
    shutil.copytree(baseline, candidate)
    engine = output / "servo-script"
    shutil.copytree(source, engine)
    for path in engine.rglob("*"):
        if path.is_file():
            path.chmod(path.stat().st_mode | 0o200)
    subprocess.run(["patch", "-p1", "-F", "0", "-i", str(PATCH)], cwd=engine,
                   stdin=subprocess.DEVNULL, check=True)
    with (candidate / "Cargo.toml").open("a") as manifest:
        manifest.write('\n[patch.crates-io]\nservo-script = { path = "../servo-script" }\n')
    # Preserve every dependency edge. `cargo update` can reselect other edges.
    registry_entry = ('[[package]]\nname = "servo-script"\nversion = "0.7.0"\n'
                      'source = "registry+https://github.com/rust-lang/crates.io-index"\n'
                      'checksum = "84ab625df5e83104d2b1ef7066f6fbc879d24be07c14438eb25f427657ed7fee"\n')
    lock_path = candidate / "Cargo.lock"
    lock = lock_path.read_text()
    if lock.count(registry_entry) != 1:
        raise RuntimeError("Expected exactly one published servo-script 0.7.0 lock entry")
    local_entry = '[[package]]\nname = "servo-script"\nversion = "0.7.0"\n'
    lock_path.write_text(lock.replace(registry_entry, local_entry, 1))
    provenance = {
        "upstream_pr": "https://github.com/servo/servo/pull/49855",
        "upstream_merge": "d4067b91beceeac11172c4ed1401c602d8338041",
        "source_package": str(source), "patch_sha256": hashlib.sha256(PATCH.read_bytes()).hexdigest(),
        "product_hashes": {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
                           for name in ["Cargo.toml", "Cargo.lock"]},
    }
    (output / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")
    print(json.dumps({"baseline": str(baseline), "candidate": str(candidate), "engine": str(engine)}))


if __name__ == "__main__":
    main()
