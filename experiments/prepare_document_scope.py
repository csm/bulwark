"""Prepare an isolated Servo 0.7.0 document-scope prototype without changing product pins."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tomllib


ROOT = Path(__file__).resolve().parent.parent
PATCH = ROOT / "experiments/phase3/patches/servo-0.7-document-scope.patch"
PACKAGES = (
    "servo", "servo-embedder-traits", "servo-constellation-traits",
    "servo-script-traits", "servo-constellation", "servo-script",
)


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--registry-source", type=Path)
    args = parser.parse_args()
    registry = args.registry_source
    if registry is None:
        cargo_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
        found = list((cargo_home / "registry/src").glob("*/servo-0.7.0"))
        if len(found) != 1:
            parser.error("Specify --registry-source containing the published Servo 0.7.0 packages")
        registry = found[0].parent
    registry = registry.resolve(strict=True)
    output = args.out.resolve()
    for tree in (ROOT, registry):
        if output == tree or output in tree.parents:
            parser.error("The output cannot replace or contain the repository or registry")
    for name in ("crates", "clj", "experiments"):
        tree = ROOT / name
        if output == tree or tree in output.parents:
            parser.error("Place --out outside source trees, for example under target/")
    if registry in output.parents:
        parser.error("Place --out outside the Cargo registry")
    # Check every source before creating the output directory.
    for name in PACKAGES:
        manifest = tomllib.loads((registry / f"{name}-0.7.0/Cargo.toml").read_text())
        if (manifest["package"]["name"], manifest["package"]["version"]) != (name, "0.7.0"):
            parser.error(f"Expected the published {name} 0.7.0 package")
    output.mkdir(parents=True, exist_ok=False)
    sources = output / "sources"
    sources.mkdir()
    for name in PACKAGES:
        destination = sources / name
        shutil.copytree(registry / f"{name}-0.7.0", destination)
        for path in destination.rglob("*"):
            if path.is_file():
                path.chmod(path.stat().st_mode | 0o200)
    source_hashes = {}
    for line in PATCH.read_text().splitlines():
        if line.startswith("--- a/"):
            relative = line.removeprefix("--- a/")
            source_hashes[relative] = sha256(sources / relative)
    subprocess.run(["patch", "-p1", "-F", "0", "-i", str(PATCH)], cwd=sources,
                   stdin=subprocess.DEVNULL, check=True)
    candidate = output / "candidate"
    candidate.mkdir()
    for name in ("Cargo.toml", "Cargo.lock"):
        shutil.copy2(ROOT / name, candidate / name)
    for name in ("crates", "clj", "experiments"):
        shutil.copytree(ROOT / name, candidate / name,
                        ignore=shutil.ignore_patterns("target", "__pycache__"))
    with (candidate / "Cargo.toml").open("a") as manifest:
        manifest.write("\n[patch.crates-io]\n")
        for name in PACKAGES:
            manifest.write(f'{name} = {{ path = "../sources/{name}" }}\n')
    # Change only the six package sources. Keep every other locked dependency edge.
    lock_path = candidate / "Cargo.lock"
    sections = lock_path.read_text().split("[[package]]")
    for name in PACKAGES:
        matches = [i for i, section in enumerate(sections)
                   if section.startswith(f'\nname = "{name}"\nversion = "0.7.0"\n')]
        if len(matches) != 1:
            raise RuntimeError(f"Expected exactly one {name} 0.7.0 lock entry")
        index = matches[0]
        package = tomllib.loads(sections[index])
        if package.get("source") != "registry+https://github.com/rust-lang/crates.io-index":
            raise RuntimeError(f"Expected a registry source for {name}")
        sections[index] = "\n".join(line for line in sections[index].split("\n")
                                    if not line.startswith(("source =", "checksum =")))
    lock_path.write_text("[[package]]".join(sections))
    provenance = {
        "servo": "0.7.0", "patch_sha256": sha256(PATCH),
        "source_hashes": source_hashes,
        "product_hashes": {name: sha256(ROOT / name) for name in ("Cargo.toml", "Cargo.lock")},
        "candidate": str(candidate),
    }
    (output / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")
    print(json.dumps({"candidate": str(candidate), "sources": str(sources)}))


if __name__ == "__main__":
    main()
