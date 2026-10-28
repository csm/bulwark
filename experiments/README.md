# Phase 0 experiments

These programs exercise integration assumptions before product implementation.
Promote their code only after the Phase 0 review.

## E2: Rust calls Clojurust

```sh
cargo run --locked -p bulwark-experiment-e2
```

The probe delivers 10,000 ordered maps in each execution mode: tree-walk and tiered.
It forces GC inside every 100th callback and again after that call returns.
It measures live objects after warmup.
It also checks exception propagation and dispatcher redefinition.
The tiered probe uses the IR interpreter. It does not install a Cranelift JIT backend.

The host resolves the dispatcher var for every call.
`apply_value` roots the function and argument values during the call.
Allocation frames end after each event, so temporary maps can become unreachable.
The host must root any returned GC value that survives another evaluation or collection.
E2 registers `bulwark.native/collect!` only for the probe. The product bootstrap does not expose this function.

Initial result on macOS arm64, Rust 1.97.1, debug build:

| Mode | Elapsed for the probe | Live objects after warmup |
|---|---|---|
| Tree-walk | 3.444 s | 5,226 throughout |
| Tiered | 422.489 ms | 9,933 throughout |

E2 passed for these execution modes. This result does not cover JIT or AOT callbacks.

## E1: Host drives async and nREPL

```sh
cargo build --locked -p bulwark-experiment-e1
python3 experiments/run_e1.py
```

The headless driver targets 60 ticks per second for 60 seconds.
The Python client evaluates forms on the loopback nREPL connection at the same time.
The driver reports async ticks, nREPL evaluations, and timing gaps.

For a native window, run:

```sh
cargo run --locked -p bulwark-experiment-e1
```

The window title shows the redraw count. Its client connection is optional for this command.
In another terminal, connect the probe to the port from the output:

```sh
python3 experiments/nrepl_probe.py PORT --seconds 50
```

For a synchronous stall, run:

```sh
python3 experiments/run_e1.py --seconds 8 --code '(do (Thread/sleep 2000) (swap! bulwark.experiment/nrepl-evals inc))'
```

The LocalSet slice is cooperative. It does not interrupt a long Clojure function.
The nREPL zero-timeout poll also permits an unbounded amount of evaluation work.

Initial 60-second headless result:

- 3,432 host ticks.
- 541 async ticks.
- 463 successful nREPL evaluations.
- 112 gaps greater than 25 ms. The maximum gap was 159.001 ms.

The run overlapped Servo compilation and used a locked Mac.
It establishes concurrent progress, but does not pass the E1 frame criterion.
The 25 ms counter is diagnostic. It does not measure every missed 16.67 ms deadline.
After the main Servo build, another 60-second run recorded 3,600 ticks and 498 nREPL evaluations.
Its async loop advanced 545 times, with no gaps greater than 25 ms and a maximum gap of 22.846 ms.
A five-second native window run delivered 291 redraws, but no nREPL client was active.
The two-second synchronous stall probe recovered and recorded a maximum gap of 2.023 seconds.
E1 still requires a 60-second native run with a client and visible stall/recovery observation.

## E3: Servo window and input

```sh
cargo build --locked -p bulwark-experiment-e3 --features servo
cargo run --locked -p bulwark-experiment-e3 --features servo -- https://servo.org
cargo run --locked -p bulwark-experiment-e3 --features servo -- https://en.wikipedia.org
```

The probe forwards pointer movement, mouse buttons, wheel events, keys, modifiers, focus, resize, and display scale changes.
The initial keyboard mapping uses shared enum names and falls back to unidentified keys.
It maps Space explicitly and preserves key locations.
IME remains outside this probe.

Q5 selects single process first. Complete this baseline before the multiprocess comparison.
Compare page rendering, input, startup, memory, and crash containment with the same fixtures.
Include JavaScript evaluation and resource interception from E4 before a default change.

For a bounded screenshot run, use:

```sh
cargo run --locked -p bulwark-experiment-e3 --features servo -- --screenshot /tmp/bulwark-servo.png https://servo.org
```

The probe saves the image after Servo reports render readiness, then exits.
It returns an error if capture fails or takes more than 30 seconds.

For a local input fixture, use its absolute `file://` URL:

```sh
cargo run --locked -p bulwark-experiment-e3 --features servo -- "file://$PWD/experiments/fixtures/input.html"
```

Click the button and enter text in the input.
The window title changes for each action.
Scroll to the end marker, then resize the window.

Initial result: Servo built on macOS arm64 and rendered servo.org and Wikipedia.
Both bounded screenshot captures passed. Visual inspection showed page text, images, and layout.
Wikipedia showed black placeholder squares for some icons, which need investigation.
The final debug executable was 397,281,016 bytes (approximately 379 MiB).
The linker reported an oversized `__eh_frame` section.
Clean build time, release size, Linux behavior, and interactive input checks remain open.

E4–E6 remain follow-up experiments in the development plan.

## Local validation

Rust formatting and Clippy passed for the workspace and the Servo probe.
The workspace test command passed. The skeleton does not yet contain Rust unit tests.
The experiment programs provide the current integration assertions.
Python syntax checks and the five-second concurrent nREPL smoke run passed.
The CI workflow remains unverified on remote runners.
