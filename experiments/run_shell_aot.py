"""Run portable shell assertions through the product's pinned AOT compiler."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def run(command, **kwargs):
    return subprocess.run(command, check=True, timeout=1200, **kwargs)


def main():
    root = Path(__file__).resolve().parent.parent
    environment = os.environ.copy()
    target = (root / environment.get("CARGO_TARGET_DIR", "target")).resolve()
    target.mkdir(parents=True, exist_ok=True)
    environment["CARGO_TARGET_DIR"] = str(target)
    environment.setdefault("CLJRS_AOT_TARGET_DIR", str(target / "aot-harness"))
    # The upstream AOT harness uses default features and its own lockfile.
    # Fetch that graph before its deliberately offline build on a fresh CI host.
    metadata = json.loads(run([
        "cargo", "metadata", "--locked", "--format-version", "1",
        "--no-default-features", "--features", "bulwark-runtime/aot-tests",
    ], cwd=root, capture_output=True, text=True).stdout)
    compiler = next(package for package in metadata["packages"]
                    if package["name"] == "cljrs-compiler")
    upstream = Path(compiler["manifest_path"]).parents[2]
    with tempfile.TemporaryDirectory(prefix="aot-dependencies-", dir=target) as directory:
        directory = Path(directory)
        dependencies = "\n".join(
            f'{name} = {{ path = {json.dumps(str(upstream / "crates" / name))} }}'
            for name in ["cljrs-types", "cljrs-gc", "cljrs-value", "cljrs-reader",
                         "cljrs-runtime", "cljrs-stdlib", "cljrs-compiler", "cljrs-async"])
        (directory / "src").mkdir()
        (directory / "src/main.rs").write_text("fn main() {}\n")
        (directory / "Cargo.toml").write_text(
            '[package]\nname = "bulwark-aot-dependencies"\nversion = "0.0.0"\n'
            'edition = "2024"\n[workspace]\n[dependencies]\n' + dependencies +
            '\ntokio = { version = "1", features = ["rt", "time", "net", "io-util"] }\n')
        shutil.copyfile(upstream / "Cargo.lock", directory / "Cargo.lock")
        version = run(["rustc", "-vV"], capture_output=True, text=True).stdout
        host = next(line.removeprefix("host: ") for line in version.splitlines()
                    if line.startswith("host: "))
        run(["cargo", "fetch", "--target", host, "--manifest-path", str(directory / "Cargo.toml")],
            cwd=root, env=environment)
    run(["cargo", "run", "--locked", "-p", "bulwark-runtime", "--features",
         "aot-tests", "--example", "shell-aot", "--", str(target / "shell-model-aot")],
        cwd=root, env=environment)


if __name__ == "__main__":
    main()
