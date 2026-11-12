# Linux test host

The Linux runner supports a persistent Ubuntu host over SSH, including a headless machine on Tailscale.
It runs the same test families as the GitHub workflow.
Each run retains a JSON summary and one log per command under `target/linux-runs/`.

## Host prerequisites

Portable checks need Rust with rustfmt and Clippy, a C/C++ compiler, Git, Python 3, and pkg-config.
The first connected host uses Ubuntu 22.04.5, Rust 1.97.1, two CPU cores, and about 4 GB RAM.
The runner defaults to one Cargo build job and one Rust test thread.
It disables debug information and incremental compilation.
These limits reduce build memory and disk use.
The small host provides correctness evidence. Laptop startup measurements remain a separate acceptance check.

The initial host inspection on 2026-11-11 established these results:

| Check | Result |
|---|---|
| SSH and public GitHub access | Passed, without SSH agent forwarding. |
| Rustfmt and Clippy | Installed. The first Linux formatting and Clippy checks passed. |
| Portable workspace tests | All 42 passed with the recorded assertion budget. |
| Complete portable suite | All 17 commands passed, including both evaluator modes, nREPL, E1, E2, E6, and E7 checks. |
| Graphics packages | Installed and configured. |
| Xvfb display | Passed. |
| Mesa GLX context | OpenGL 4.5, Mesa 23.2.1, llvmpipe with LLVM 15.0.7. |

The GLX probe created a small offscreen buffer under Xvfb with software rendering.
It establishes context creation, not Servo acceptance or hardware GPU behavior.

The initial browser-history assertion exceeded its ten-second budget in both parallel and serial workspace runs.
The isolated test passed in both execution modes.
A later timing check recorded 8.57 seconds for TreeWalk and 8.63 seconds for Tiered mode.
The runner therefore supplies an explicit 60-second assertion budget for this small host.

Servo checks also need the system packages from the Linux CI workflow.
If the account requires a sudo password, install these packages from an interactive SSH session:

```sh
sudo apt-get update
sudo apt-get install -y clang llvm cmake ninja-build pkg-config xvfb xauth \
  libfontconfig1-dev libfreetype-dev libglib2.0-dev libudev-dev \
  libx11-dev libxcursor-dev libxi-dev libxrandr-dev \
  libgl1-mesa-dev libegl1-mesa-dev libdbus-1-dev libunwind-dev \
  libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev
```

The runner does not install packages or change system configuration.
It adds `~/.cargo/bin` to its process environment, so a noninteractive SSH shell can find Rust.
Tailscale supplies connectivity. The existing SSH account and keys supply authentication.
SSH agent forwarding is unnecessary for public dependency sources.

## Run checks

1. Copy or fetch the exact source revision into a dedicated checkout on the Linux host.
2. Run the portable suite:

   ```sh
   python3 experiments/run_linux_checks.py --suite portable
   ```

3. Run the AOT suite:

   ```sh
   python3 experiments/run_linux_checks.py --suite aot
   ```

4. After the system packages are available, run the Servo suite:

   ```sh
   python3 experiments/run_linux_checks.py --suite servo
   ```

`--suite all` runs portable, Servo, and AOT checks in that order.
`--list` prints the planned commands without a Linux host or a build.
`--jobs N` changes Cargo parallelism. `--timeout SECONDS` sets a deadline for each command.
The default deadline is one hour per command.
`RUST_TEST_THREADS` can override the default Rust test parallelism.
The runner sets `BULWARK_TEST_PROGRAM_TIMEOUT_SECS=60` for the bulk Clojure assertion programs.
The ordinary harness default remains ten seconds without this environment variable.
The summary records the selected budget. Each program reports its execution mode and duration.
The runner retains Rust test output through `RUST_TEST_NOCAPTURE=1`.
The existing two-second shutdown and cleanup deadlines remain unchanged.
The first failed command stops the run and retains its output.
A deadline terminates that command's process group, including its build or test children.

The Servo suite uses Xvfb and Mesa software rendering.
It covers the native host, retained webviews, both Clojure execution modes, nREPL, and worker shutdown.
Physical keyboard, pointer, IME, compositor, and hardware GPU checks remain separate.

The same runner works through a noninteractive SSH connection:

```sh
ssh user@host 'cd ~/src/bulwark && python3 experiments/run_linux_checks.py --suite portable'
```

The connected test machine uses a dedicated checkout under `~/src/bulwark-linux-tests/checkout`.
The first run uses Phase 2 commit `79211e4` plus the new runner file.
Its summary records the untracked runner and the SHA-256 of that file.
The later run also records the modified Rust harness for the configurable assertion budget.
The source arrived as a Git bundle, without a copy of the developer's credentials or SSH agent forwarding.
The initial remaining checks run in sequence after the portable suite: AOT, then Servo.
Their queue status is in `~/src/bulwark-linux-tests/queue.json` on the host.
The queue stops after a failed suite and retains its logs.
The passing portable report and command logs are also available locally under `target/linux-runs/nuc-portable-20261111T214254Z/`.

## Keep runs reproducible

The runner records the checked-out commit in each summary.
Use a dedicated checkout and retain its build directory between runs.
Do not copy macOS build artifacts onto the Linux host.
If a run uses an uncommitted runner change, record that change alongside the source revision.
Keep host addresses and SSH credentials outside repository configuration.
The connected host's SSH target and checkout paths are in the ignored local file `target/linux-host.json`.
