# Bulwark Development Plan

Status: active v0.11 (2026-11-07), Phase 0 complete. Phase 1 implementation is next. Derived from `docs/bulwark.md`, Section 9 ("Next Steps"),
which calls for decomposing the high-level plan into concrete, independently verifiable phases.

This document is the working plan. `docs/bulwark.md` remains the statement of vision and
principles; where the two disagree, the vision document wins and this plan should be amended.

---

## 1. What this plan covers

Section 9 of the vision document asks for five things. Each maps to a section below.

| Vision §9 asks for | Section here |
|---|---|
| Architecture and API decisions required before implementation | §3 Decisions |
| Dependencies between Servo integration, Clojurust runtime capabilities, and the browser shell | §4 Dependency map |
| Technical experiments needed to resolve high-risk assumptions | §5 Experiments (Phase 0) |
| Specific deliverables and acceptance criteria for each phase | §6 Phases |
| A minimal vertical slice demonstrating the end-to-end architecture | §6, Phase 1 |

---

## 2. Ground truth on the dependencies

The plan is only as good as its assumptions about Servo and Clojurust. This section records
what was verified against upstream sources on 2026-10-27, and what it implies. Re-verify at the
start of Phase 0; both projects move quickly.

Bootstrap update: the workspace pins the published Servo 0.7.0 crate and Clojurust commit
`b847a29b46e2b6b3fbe624f6d2394cb656ea5882`. The initial main-branch inspection remains
background context. [Upstream notes](upstream.md) record the pinned-source inspection.
The [Phase 0 exit review](phase-0-exit.md) records accepted evidence, fallbacks, and Phase 1 entry.
Section 10 preserves earlier experiment results.

### 2.1 Servo (the `servo` crate)

Verified against `components/servo/{lib.rs,webview.rs,webview_delegate.rs,servo_delegate.rs}`
on `servo/servo` `main`, and `ports/servoshell`.

- The engine is published to crates.io as the `servo` crate (0.1.0, April 2026). Upstream has
  stated an intent to do LTS releases for embedders. This is the integration point; Bulwark
  should not depend on Servo's internal component crates.
- Embedding model is delegate-based, not message-based:
  - `ServoBuilder` builds a `Servo`; the embedder implements `EventLoopWaker` and must call
    `Servo::spin_event_loop` whenever woken and after any `WebView` request.
  - `WebViewBuilder::new(&servo, Rc<dyn RenderingContext>)` with `.delegate(..)`, `.url(..)`,
    `.user_content_manager(..)`, `.build()`. A `WebView` has `load`, `reload`, `go_back`,
    `go_forward`, `can_go_back`, `url`, `page_title`, `load_status`, `resize`, `show`/`hide`,
    `set_focused`, `notify_input_event`, `notify_scroll_event`, `paint`, `take_screenshot`,
    and `evaluate_javascript(script, FnOnce(Result<JSValue, JavaScriptEvaluationError>))`.
  - `WebViewDelegate` has 31 methods. The important ones for Bulwark: `notify_url_changed`,
    `notify_page_title_changed`, `notify_load_status_changed`, `notify_history_changed`,
    `notify_new_frame_ready`, `notify_closed`, `notify_crashed`, `request_navigation`
    (allow/deny), `request_create_new` (window.open), `request_permission`,
    `request_authentication`, `load_web_resource` (intercept any HTTP(S) load and supply a
    response), `show_embedder_control` (select/color/file pickers, IME, alert/confirm/prompt,
    context menus all arrive here), `show_console_message`, `show_notification`.
  - `ServoDelegate` covers engine-level events: errors, devtools server start and connection
    requests, non-webview resource loads, console messages.
- Rendering contexts: `WindowRenderingContext`, `OffscreenRenderingContext`,
  `SoftwareRenderingContext`, or a custom `RenderingContext`. The delegate must handle
  `notify_new_frame_ready` by calling `WebView::paint` and `RenderingContext::present`.
- `WebView` holds an `Rc<dyn RenderingContext>`; the embedding API is single-threaded by
  construction. All Servo calls happen on the thread that owns `Servo`.
- There is **no structured DOM access API**. The only script-level hook is
  `evaluate_javascript`, plus `UserContentManager` for injected scripts. Any DOM API Bulwark
  exposes to Clojurust must be built on injected JS and a message channel (see §3, D5).
- The reference embedder (`servoshell/desktop`) uses `winit` for windowing, `egui` +
  `egui-winit` + `egui_glow` for chrome, and `surfman` for GL surfaces. It builds Servo with
  `multiprocess`, `js_jit`, `default_web_features`, `devtools`, and `media-gstreamer`.

### 2.2 Clojurust (`csm/clojurust`)

Verified against the README, `TODO.md`, `examples/rust-interop/src/main.rs`,
`crates/cljrs-runtime/src/runtime.rs`, `crates/cljrs-async/src/lib.rs`,
`crates/cljrs-nrepl/src/lib.rs`, `crates/cljrs-interop/src/lib.rs`.

- Rust-hosted Clojure dialect, reads `.cljrs` and `.cljc`, platform key `:cljrs`. Early stage
  by its own description ("expect rough edges") but far along: tiered execution (tree walker
  → IR interpreter → Cranelift JIT), AOT to native binaries, non-moving mark-sweep GC,
  1,162 Rust tests plus the `clojure-test-suite` assertions.
- Embedding, as the `rust-interop` example does it:
  `Runtime::builder().execution_mode(ExecutionMode::Tiered).build()`, then
  `cljrs_stdlib::install(&runtime)`, then `Env::new(runtime.into_globals(), "user")`; forms are
  read with `cljrs_reader::Parser` and run with `eval(form, &mut env)`.
  `RuntimeBuilder::builtin_source(ns, src)` embeds a namespace's source so `require` resolves
  it without a file. This is how Bulwark's own `.cljrs` will ship inside the binary.
- Exposing Rust to Clojure: `#[cljrs_interop::export(ns = "...")]` plus `Registry`, or
  `wrap_fn0..3`/`wrap_fn_variadic` with `FromValue`/`IntoValue` marshalling, or manual
  `globals.intern(ns, ..)` of a `Value::NativeFunction`. Opaque Rust state is held as a
  `NativeObject` via `gc_native_object`. `globals.mark_loaded(ns)` stops `require` from
  searching the filesystem.
- **Calling Clojure from Rust has a public path in the pinned revision.**
  `cljrs_runtime::env::apply::apply_value` accepts a function value, argument values, and an
  explicit `Env`. It roots the callee and arguments before its GC safepoint.
  `tiered::invoke` requires an active evaluation context and is unnecessary for host calls.
  E2 passed in tree-walk and tiered modes. JIT and AOT remain unverified.
- Threading: `GcPtr`, `Value`, `Env`, and `GlobalEnv` are **not `Send`**. The runtime lives on
  one thread. `cljrs-async` runs core.async (`go`, `chan`, `<!`, `>!`, `alts!`, `timeout`) on
  a Tokio `current_thread` runtime with a `LocalSet` that the host must drive on that same
  thread; `cljrs_async::init(&globals)` must be called inside that `LocalSet`. The crate
  exposes `spawn_future`/`await_value`, and has `isolate`, `isolate_channel`, and
  `worker_pool` modules whose contents were not verified.
  At `c6199fae`, native `go` and `^:async` bodies run on stackful fibers within that LocalSet.
  Waits park those fibers, including through ordinary helper calls. The Rust bridge retains ordinary async futures.
- nREPL: `cljrs_nrepl::start(Config, globals) -> Server` spawns its own network thread but
  evaluates on the calling thread. `Server::into_poller()` yields a `Poller` with
  `poll(&mut eval_form)` / `poll_timeout(..)` for hosts that run their own loop. This is
  the legacy E1 path. The new `spawn_pending` API starts jobs on fibers without waiting for completion.
  E7 now uses `spawn_pending_budget` and explicit accepted-work limits at `b847a29b`.
- GC caveats from `TODO.md`: finalization hooks, weak references, and write barriers are
  deferred. Native resources held from Clojure (webviews, buffers) therefore cannot rely on
  GC finalizers for cleanup; lifetimes must be explicit, which the vision's risk table already
  prescribes ("handle-based APIs with explicit lifetimes and invalidation").
- Other gaps relevant here: `cljrs-tx` (STM/refs) is "initial"; `locking` deferred; LSP is
  syntactic only; the calling convention for Clojure → Rust is listed as still open.

### 2.3 Scope note

An earlier draft of the vision document read "Clojurust (Corium)". Corium is an unrelated
project (a Datomic-style database by the same author) that entered the text by way of an AI
assistant's memory; it is not part of this project. The aim is a Clojurust programmable
interface for a Servo-powered browser, and nothing in this plan depends on Corium.

### 2.4 Implications that drive the architecture

1. Servo and Clojurust are both single-thread-affine, but they can own different threads.
   D1 puts `winit`, Servo, native resources, and rendering on the main thread.
   A dedicated thread owns Clojurust, its Tokio `LocalSet`, and E7's nREPL integration.
   Owned requests, replies, and events cross channels. E7 validates this mechanism with real Servo.
2. Native Clojure fibers park at supported waits and yield at evaluator safepoints.
   A raw blocking native call can still stop commands and hooks while Servo continues independently.
   Arbitrary native code has no preemption guarantee.
3. DOM access must be built by Bulwark, as injected JS plus a message protocol, because
   Servo exposes only `evaluate_javascript` and `UserContentManager`.
4. Native-handle lifetime management is Bulwark's responsibility: handles are revocable
   tokens, not GC-managed objects.
5. Bulwark will exercise Clojurust embedding paths that require experiments.
   E2 uses an existing public API without an upstream patch.
   E6 confirms mid-form nREPL interruption and recovery. Cooperative scheduling still permits unbounded synchronous work.
   E7's native await, idle GC, and nREPL admission regressions pass at `b847a29b`.
   The [gap register](clojurust-async-gaps.md) records upstream adoption and the remaining product limits.
   The owner approved preparation of required upstream Clojurust fixes under Q4.

---

## 3. Architecture and API decisions required before implementation

D1–D6 now have accepted ADRs for the initial product.
The [exit review](phase-0-exit.md) links all six decisions and their evidence.
Later decisions remain phase-specific and can change as implementation supplies new evidence.

**D1. Threading and event-loop ownership.**
Accepted Phase 1 ownership contract: separate engine and runtime threads, per the owner's request.
The main thread hosts `winit`, Servo, native chrome, and rendering.
The dedicated Clojurust thread runs its own Tokio executor and LocalSet continuously.
Native browser calls return futures backed by bounded requests and one-shot replies.
Engine events cross a bounded ordered queue and reach Clojure through an async event loop.
Servo wakeups and command arrivals wake the main event loop. No engine callback evaluates Clojure on the main thread.
E7 validates native window and runtime-thread nREPL integration under finite load.
Upstream idle GC and admission mechanisms now pass their regression probes.
D1 defines product queue, payload, overload, and shutdown policies. Phase 1 must implement them.
The [D1 ADR](adr/0001-shell-thread.md) and [async bridge report](async-bridge.md) record the revised direction.
E1 remains the historical comparison for the earlier shared-thread proposal.

**D2. Rust ↔ Clojure boundary is data, not objects.**
Native capabilities are exposed as a small set of namespaced native functions
(`bulwark.native/*`) that take and return plain Clojure data (maps, vectors, keywords,
strings, numbers) plus opaque handle values. Engine operations return futures whose results contain that data.
Rust converts arguments and replies on the Clojurust thread. No GC or Servo object crosses a channel.
Engine events are delivered as event maps
(`{:event/type :page/url-changed :webview 7 :url "https://..."}`) into a single Clojure
dispatcher function from a Clojure async event loop. Routing, hooks, and modes remain in Clojure.
E2 remains the supported local host callback mechanism. The engine thread never calls it directly.

**D3. Handles are explicit, revocable tokens.**
A webview, buffer, window, download, or subscription is identified by an integer or keyword
id issued by Rust. Rust owns a registry mapping id → native resource. Clojure holds only ids.
D3 selects monotonic identifiers that never repeat within a process. Exhaustion returns an error instead of wrapping.
Invalidation is explicit (`(bulwark.native/webview-close id)`), and using a dead id returns a
structured error, never UB. This follows directly from the GC caveats in §2.2.

**D4. Engine abstraction boundary.**
Define a Rust trait `Engine` (or a module boundary) covering: create view, destroy view,
navigate, history, resize/show/hide/focus, input forwarding, evaluate script, inject user
content, intercept resource loads, and an event stream. The Servo implementation is the only
one, but no Servo type crosses into the Clojure-facing API layer. Keep this boundary cheap:
one crate `bulwark-engine-servo` behind the `bulwark-engine` interface, no premature generality.
The [D4 ADR](adr/0004-engine-boundary.md) records ownership, scope, and completion semantics.

**D5. DOM API strategy.**
Structured DOM access uses a Bulwark-owned JS agent injected through `UserContentManager`.
Requests use `evaluate_javascript`, and responses return through its callback.
Larger results use JSON and become EDN at the runtime boundary.
E4 passed this channel and a same-origin HTTPS resource interception exchange.
The agent shares the page's JavaScript world. It does not provide isolation or authenticate messages.
Resource interception remains a possible channel for page events, subject to product security work.
Node references will use document generations and become invalid after navigation.
The [D5 ADR](adr/0005-dom-channel.md) records the channel choice and remaining limits.

**D6. Chrome and non-web buffer rendering.**
Two viable options, with a recommended sequence:

- Option A: `egui` chrome, as servoshell does. Clojure describes the UI as data (hiccup-like
  vectors); Rust renders it with `egui`. Fast, no security concerns, but a second UI model.
- Option B: non-web buffers are privileged Servo webviews loading `bulwark://` URLs served by
  `load_web_resource` interception, with HTML generated from hiccup in Clojure. One rendering
  model, but depends on Servo maturity and needs a hardened privileged bridge.

Accepted under Q3: Option A for the minimal chrome needed in Phases 1 and 2 (echo area, modeline,
minibuffer/command palette, REPL buffer). Option B for user-defined buffer types from Phase 3,
once E5 shows multiple webviews can coexist and the bridge can be made safe. Window splits
and layout are pure Clojure data in both options; Rust only receives rectangles.

**D7. Persistence.**
Phase 1 and 2: EDN files under the profile directory, written atomically. Phase 4 chooses
the daily-driver store; the default is SQLite (proven for browser profiles, small, no server),
with an append-only EDN log kept as the fallback if a Rust SQLite binding proves awkward to
expose through `bulwark.native`. Decision criteria: startup cost, binary size, and write
latency, which must be off the shell thread either way. Snapshot and replay features in
Phase 5 are designed on top of whichever store is chosen, not the other way around.

**D8. Security tiers and enforcement point.**
Three principals, enforced in Rust at the native boundary, as the vision requires:

| Tier | Example | Can |
|---|---|---|
| Trusted config | `~/.bulwark/init.cljrs`, nREPL | everything |
| Capability-restricted extension | packages loaded from a registry | only what its declared capability set allows |
| Untrusted web content | page JS, injected agent responses | nothing beyond web platform APIs; agent responses are data |

Mechanism: each Clojure evaluation context carries a capability set; native functions check
it. Clojurust has no built-in sandbox for this, so initially enforcement is per-call in Rust
plus loading extensions in a separate namespace prefix with a restricted native table.
Full isolation (separate isolates) is a Phase 5 question.

**D9. Repository and crate layout.**
Cargo workspace:

```
bulwark/
  Cargo.toml                 workspace
  crates/
    bulwark-engine/          Engine trait + event types (no Servo dependency)
    bulwark-engine-servo/    Servo implementation
    bulwark-runtime/         Clojurust embedding: runtime setup, native fn registry, dispatcher bridge
    bulwark-shell/           winit app, event loop, chrome, main()
  clj/
    bulwark/                 the Clojure side: core, buffers, windows, commands, keymaps, modes
    bulwark/default/         default configuration (what a user gets with no init file)
  docs/
    bulwark.md               vision
    development-plan.md      this file
    adr/                     decisions
```

`.cljrs` sources ship embedded via `RuntimeBuilder::builtin_source`, with a dev flag to load
from disk for live reloading.

**D10. Upstream pinning.**
Depend on `servo` from crates.io pinned to an exact version, and on `csm/clojurust` as a git
dependency pinned to a commit. Bump deliberately, once per phase, with a changelog note. Any
change Bulwark needs in Clojurust goes upstream as a PR first; Bulwark does not fork.

**D11. Naming and API stability policy.**
`bulwark.native/*` is the only namespace Rust populates and is considered unstable until
Phase 3 exit. Everything users are expected to call lives in `bulwark.*` Clojure namespaces,
which wrap the native layer and can be kept stable while the native layer changes.

---

## 4. Dependency map

```
            ┌──────────────────────┐
            │ D1 ownership probe   │
            │ E7 async channels    │
            └──────┬───────────────┘
                   │
   ┌───────────────┼──────────────────┐
   ▼               ▼                  ▼
E2 Rust→Clj    E3 Servo embed     E5 multi-webview
callbacks      (window+paint)     + layout rects
   │               │                  │
   └──────┬────────┘                  │
          ▼                           │
   Phase 1: vertical slice ◄──────────┘ (E5 can slip to Phase 2)
          │
          ▼
   Phase 2: shell (buffers, windows, commands, keymaps, modes)
          │
          ├── E4 DOM channel ──► Phase 3: web integration
          │                          │
          │                          ▼
          └───────────────────► Phase 4: daily driver  ◄── D7 persistence decision
                                     │
                                     ▼
                                Phase 5: advanced
```

Hard dependencies:

- Phase 1 needs a completed D1 ownership review plus E2 and E3.
  E7 is the ownership probe for the separate-thread direction. E1 remains evidence for the earlier shared-thread proposal.
  The review must record upstream fixes for E7's async gaps and the product queue policies.
  The [async gap register](clojurust-async-gaps.md) defines the adoption gates and regression requirements.
- Phase 2 needs E5 only for split windows showing multiple web pages at once; a Phase 2 with
  one visible webview at a time (Emacs with a single window) is acceptable fallback.
- Phase 3 needs E4.
- Phase 4 needs the D7 decision and Servo compatibility results; it is the phase most exposed
  to upstream Servo limitations and should not be scheduled to a date.

Clojurust capabilities each phase relies on:

| Phase | Clojurust capability | Status |
|---|---|---|
| 1 | embed runtime, native fns, `builtin_source`, nREPL poller | documented |
| 1 | Rust → Clojure dispatcher call | public API, E2 passed in tree-walk and tiered modes |
| 1 | core.async on host-driven `LocalSet`, fiber-backed since 0.2.34 | headless host integration verified (E1, E6) |
| 1 | core.async on an independent runtime thread | E7 verifies real Servo, idle suspension, bounded nREPL admission, overload, and recovery |
| 2 | dynamic vars, protocols, multimethods, metadata/docstrings for introspection | implemented |
| 2 | `require` of embedded and on-disk namespaces, reload | implemented |
| 3 | JSON/EDN conversion at scale, `NativeObject` for node handles | implemented |
| 4 | file I/O and networking off-thread (`cljrs-io`, `cljrs-net`) | implemented |
| 5 | isolates / worker pools for background tasks | present in `cljrs-async`, unverified |
| 5 | refs/STM for shared state (if wanted) | initial |

---

## 5. Experiments (Phase 0)

Each experiment is time-boxed, has a pass criterion, and names what changes if it fails.
They can run in parallel except where noted. Code lives in `experiments/` and is thrown away
or promoted deliberately; none of it is "the product".

**E1. Shell thread loop.** Build a `winit` app that owns a Clojurust runtime, drives a Tokio
`LocalSet` with `cljrs_async::init`, polls a `cljrs_nrepl` `Poller`, and keeps a 60 Hz redraw
alive while a `(go (loop [] (<! (timeout 100)) ...))` runs and an nREPL client evaluates
forms. Pass: no missed frames over 60 s with the go-loop and nREPL both active; a `(Thread/sleep)`-equivalent
busy loop in Clojure visibly freezes the UI (confirming the hazard is real and must be
budgeted). Time box: 3 days. If it fails: investigate running Clojurust on a dedicated thread
with a cross-thread command/event queue (costs latency and complicates Servo interaction).

Bootstrap status: the driver and concurrent loopback nREPL client exist.
A 60-second headless repeat reached 3,600 ticks with no gaps greater than 25 ms.
Native 60-second runs now report runtime ticks, redraw callbacks, and skipped deadlines.
The full native criterion remains open. The dedicated-thread fallback exists as an experiment.
Current measurements separate window startup from the interval between redraw callbacks.
The [scheduling report](scheduling.md) records its limits and the remaining D1 decisions.
Before 0.2.34 the async API required `(await (timeout 100))` inside `go`.
This experiment now serves as the historical comparison for D1. E7 investigates the separate-thread direction.


Fiber status (2026-11-02): the Clojurust 0.2.34 pin adds virtual threads. E1 now uses `(<! (timeout 100))`.
Startup and nREPL forms run on root fibers. A 60-second Linux headless run had 497 nREPL evaluations
and 2 gaps over 25 ms. A parked nREPL form still blocks the frame (csm/clojurust#447).

**E2. Rust → Clojure callback.** From Rust, look up a var (e.g. `bulwark.core/dispatch!`) and
invoke it with a map argument, 10,000 times, with the GC active, without leaks or crashes.
Pass: a documented or newly contributed public API in `cljrs-runtime` for `apply` of a
`Value` function from host code, with GC-root safety stated. Time box: 3 days plus upstream
PR time. If it fails: Rust pushes events into a core.async channel instead (verify
`cljrs-async` `channel` module supports host-side puts), and Clojure pulls; this is
acceptable and may even be preferable.

Bootstrap status: passed using `env::apply::apply_value` in the pinned revision.
Both execution modes delivered 10,000 ordered maps with 100 in-callback collections and 100 between calls.
Live objects stayed constant after warmup. Errors and live redefinition also passed.
The existing API roots its arguments. No upstream patch was necessary.

**E3. Servo embed.** Using the `servo` crate, open a `winit` window, create one `WebView`
with `WindowRenderingContext`, load a URL, paint on `notify_new_frame_ready`, forward
keyboard and mouse. Record build time, binary size, and which Servo features are required.
Pass for the initial macOS milestone: renders `https://servo.org` and `https://en.wikipedia.org`
interactively on macOS. Repeat on Linux as follow-up work under Q2.
Time box: 3 days (mostly build time). If macOS embedding fails: the project is blocked on upstream;
stop and report.

Bootstrap status: the public-API window probe builds on macOS arm64.
Servo rendered servo.org and Wikipedia. Bounded screenshot captures passed for both pages.
The clean debug target built in 2m 30s. A release build took 194.83 s and produced a 126 MiB executable.
These measurements used cached sources and include concurrent runtime checks.
Native fixture input, scrolling, editing, resize, and close checks passed on 2026-11-06.
Link navigation also passed on servo.org and Wikipedia. Wikipedia's icon rendering remains incomplete.
The [native macOS report](native-macos.md) records the observations and remaining limits.
Linux remains follow-up work.

Q5 sets the sequence: complete the single-process baseline, then compare multiprocess Servo.
The comparison will cover rendering and input, startup and memory costs, crash containment,
JavaScript evaluation, and resource interception. Record the results before changing the default.
Multiprocess support does not gate the initial single-process milestone.

**E4. DOM channel.** Inject an agent via `UserContentManager`; measure round-trip latency of
`evaluate_javascript` for (a) a trivial expression, (b) serializing a 5,000-node subtree to
JSON; test the `load_web_resource` intercept as an alternative page → host channel. Pass: a
chosen channel with p50 latency under 5 ms for (a) and a documented upper bound for (b).
Time box: 3 days. Runs after E3.

E4 passed on macOS on 2026-11-06 with a local intercepted HTTPS fixture and an injected agent.
The trivial call had a 49.292 µs p50 across 100 samples.
The 5,000-element subtree had a 24.741 ms p50 and a 30.444 ms sample maximum across 30 samples.
Resource interception delivered a page message and acknowledgement with asserted Unicode data.
[D5](adr/0005-dom-channel.md) selects JavaScript evaluation callbacks for DOM request results.
The [DOM channel report](dom-channel.md) records the payload, repeat run, and security limits.

**E5. Multiple visible webviews.** Two `WebView`s visible simultaneously in one window at
different rectangles, both animating. Try: one rendering context with show/hide and resize
per frame; two `OffscreenRenderingContext`s composited by the shell. Pass: two pages
interactive side by side at acceptable frame rate. Time box: 4 days. Runs after E3. If it
fails: Phase 2 ships with one visible web buffer per frame, with non-web buffers in `egui`.

Exit disposition: E5 remains deferred. The initial product uses one visible webview per native window and egui for non-web panels.
This accepted fallback permits Phase 1 entry. Web content splits require a passing E5 result.

**E6. Runtime time budgeting.** Instrument the E1 loop: cap Clojure dispatch
per frame, measure how core.async `go` blocks yield, and test whether a long synchronous
Clojure call can be interrupted (nREPL `interrupt` exists; does it reach the interpreter
mid-form?). Pass: a written policy for what runs where, and an `interrupt` that works from a
second nREPL connection. Time box: 2 days. Runs after E1.

Progress: `experiments/run_e6.py` proves mid-form interruption from a second connection and subsequent session and async recovery.
It also measures a blocking `go` task and a burst of queued evaluations.
Both exceed the host's nominal time slice.
The [scheduling report](scheduling.md) defines the provisional policy.
Budgeted nREPL admission now passes in E7. D1 records the revised runtime policy.
Phase 1 must enforce admission between product event callbacks.
Under separate ownership, runtime stalls do not require equivalent Servo stalls.

Fiber status (2026-11-02): `experiments/e6-fibers` measures the 0.2.34 fiber runtime on a 60 Hz loop.
Parked waits and offloaded Rust calls do not delay frames. Slice yields bound CPU-bound fibers.
Carrier work, root-fiber work, and thread-blocking Rust calls still freeze the loop.
Interrupts from a second connection stop running evaluations but not parked ones (csm/clojurust#446).
The fiber policy is in [D1](adr/0001-shell-thread.md). The interrupt criterion remains open for parked evaluations until #446 is fixed.

These 0.2.34 results are historical. E7 passes parked interruption and cross-session progress at both `c6199fae` and `b847a29b`.
The E6 fiber probe remains available as the shared-thread comparison.


**E7. Async user code across a thread boundary.** Run Servo on the main thread and Clojurust on a dedicated LocalSet thread.
Use bounded owned requests, one-shot replies, ordered events, and async Clojure browser helpers.
Pass: real Servo create/evaluate/close requests, reply correlation under concurrency, overload and stale-handle errors,
runtime async progress, and continued Servo painting during a deliberate runtime stall.
Record idle CPU, request latency, shutdown behavior, and gaps in the pinned async evaluator.
Time box: 2 days. Native `winit` integration now passes its workflow and lifecycle probes.
nREPL integration now passes finite-load concurrency and shutdown probes.
The `b847a29b` update supplies budgeted admission and accepted-work limits. The overload and idle regressions now pass.

Initial result: the real Servo probe passed its mechanism assertions.
During a 200 ms runtime block, Servo completed 57 turns and painted six frames. The page timer also advanced.
The mock reverses reply batches to exercise correlation independently of completion order.
Idle awaits consume CPU, and await under a keyword call blocks. Both have repeatable probes.
The [async bridge report](async-bridge.md) records the contract, results, and remaining decisions.

The 2026-11-06 dependency update adopts native stackful fibers for `go` and `^:async` bodies.
The nested-await regression now passes, and `Thread/sleep` parks inside a fiber.
The real Servo repeat painted 21 frames during an explicitly blocking 200 ms Rust call.
That revision replaced busy await loops but retained a busy GC service.
The `b847a29b` update corrects the GC service and adds bounded nREPL admission.
The current [gap register](clojurust-async-gaps.md) records the passing regressions and remaining product limits.

The native continuation uses `EventLoopProxy` for command, Servo, and worker-completion wakeups.
The final native workflow completed 119 requests and presented 20 frames during its raw runtime stall.
Six lifecycle cases passed, including intentional close, runtime error, panic, and unresponsive-call shutdown.
Both drivers now bound the wait before a worker join.
The [native bridge report](native-async-bridge.md) records the contract and process-exit fallback.

The nREPL continuation uses `spawn_pending` on the runtime's existing LocalSet.
Mock, software Servo, and native Servo passed cross-session browser calls, session order, interruption, redefinition, and shutdown.
The native window presented 17 frames during an infinite Clojure evaluation in another session.
The [nREPL report](nrepl-runtime.md) records measured durations and the later passing overload assertions.

Phase 0 exit: D1 ownership review, E2, and E3 complete; E4, E5, E6 either passed or have a recorded fallback; the
ADRs for D1–D6 written with final choices.

**Exit review complete, 2026-11-07.** All required gates pass or have the recorded fallback permitted by this criterion.
The [exit review](phase-0-exit.md) records E1 replacement by E7, E5 deferral, E6 policy, accepted ADRs, and remaining limits.
Phase 1 implementation can begin. Its product acceptance criteria remain open.

---

## 6. Phases, deliverables, acceptance criteria

Each phase produces a working system. "Verifiable" means a person can check the criterion by
running the build and following written steps; where possible, it is also an automated test.

### Phase 1: Vertical slice (vision §8, "Initial Proof of Concept")

Goal: validate that a useful browser shell can be written primarily in Clojurust with Rust
and Servo providing mechanisms.

Deliverables:

1. Workspace per D9 with `bulwark-engine`, `bulwark-engine-servo`, `bulwark-runtime`,
   `bulwark-shell`, and `clj/bulwark/core.cljrs`.
2. Native API v0 (`bulwark.native`): `webview-create`, `webview-close`, `navigate`, `reload`,
   `back`, `forward`, `webview-info` (url, title, load status, can-go-back/forward),
   `eval-js`, `log`, `quit`, and `set-echo!` (text in the egui echo area).
3. Event delivery: the runtime's async event loop calls `bulwark.core/dispatch!` with event maps for url-changed,
   title-changed, load-status-changed, history-changed, closed, crashed, and key input.
4. Clojure side: a command registry (`defcommand` with docstring), a keymap with
   Emacs-style sequences (`C-x C-f` style parsing), `bulwark.hooks` with `add-hook!`, and a
   dispatcher that routes keys to commands and events to hooks.
5. nREPL on `127.0.0.1` with port file in the profile dir, enabled by flag.
6. One meaningful behavior entirely in Clojure. Suggested: "readable history" — a hook on
   `:page/title-changed` that appends `{:url :title :at}` to an atom and a command
   `history-search` that prompts in the minibuffer and navigates to the chosen entry.

Acceptance criteria:

- A1. `cargo run` opens a window showing a web page rendered by Servo. (vision §8.1)
- A2. Connecting an nREPL client to the port in the port file and evaluating
  `(bulwark.core/navigate! "https://example.com")` navigates the visible page. (§8.2, §8.3)
- A3. `(add-hook! :page/title-changed (fn [e] (set-echo! (:title e))))` from the REPL causes
  the echo area to show titles on subsequent navigations, without restart. (§8.4)
- A4. `(defcommand reload-twice ...)` followed by `(bind-key! "C-c r" 'reload-twice)` from the
  REPL makes the key sequence work immediately. (§8.5)
- A5. The behavior in deliverable 6 works end to end with no Rust code specific to it. (§8.6)
- A6. Rust code in `bulwark-shell` and `bulwark-runtime` contains no browsing policy: a
  reviewer can list every native function and each is a mechanism (do X to view N), not a
  behavior (when Y happens, do Z).
- A7. A 2-second synchronous Clojure loop pauses runtime commands while Servo input and rendering continue.
  The runtime then recovers without a crash or invalid handle state. Document the command queue's behavior during the stall.
- A8. CI builds on Linux and runs: Rust unit tests, Clojure tests in `clj/` under the
  interpreter, and a headless smoke test that starts the shell with
  `SoftwareRenderingContext`, loads a local HTML file, and asserts a title-changed event
  reached Clojure.

Exit review question: did anything in Phase 1 have to be written in Rust that the vision says
should be Clojure? If yes, either the native API is missing a mechanism or the vision needs
adjusting; decide which before Phase 2.

### Phase 2: Programmable browser shell

Goal: the Emacs-inspired application model, with default browser behavior moved into Clojure.

Deliverables:

1. Buffers: `bulwark.buffer` with a buffer registry, buffer-local variables, `:major-mode`
   and `:minor-modes`, buffer kinds `:web`, `:repl`, `:text` (read-only text, used for
   help/docs), with `:web` buffers backed by native webview handles.
2. Windows and layouts: `bulwark.window` describing a tree of splits as data; a layout
   function that produces rectangles; Rust applies rectangles to webviews and egui panels.
   Fallback if E5 failed: one visible web buffer, others hidden; splits allowed only for
   non-web buffers.
3. Modes: `define-major-mode`, `define-minor-mode`, mode keymaps with precedence
   (minor > major > global), mode hooks.
4. Command palette (`M-x`) with fuzzy completion over commands, docstrings shown; minibuffer
   with completion for buffers, history entries, and URLs.
5. Introspection: `describe-command`, `describe-key`, `describe-variable`, `apropos`,
   rendered into a `:text` buffer. Every native function has a docstring and appears in
   `apropos`.
6. Default configuration in `clj/bulwark/default/`: new tab, close tab, switch buffer, back,
   forward, reload, focus address (minibuffer), find-in-page (via injected JS for now), zoom,
   open link in new buffer, basic `window.open` handling via `request_create_new`.
7. Init file: `~/.bulwark/init.cljrs` loaded at startup, `reload-init` command, errors shown
   in a `*Messages*`-style buffer rather than crashing.
8. Event hooks for navigation, load, focus, buffer create/kill, window change.
9. REPL buffer inside the browser (a `:repl` buffer, egui-rendered) in addition to nREPL.

Acceptance criteria:

- B1. All of Phase 1's criteria still pass.
- B2. A user with no init file gets: address entry, tabs as buffers, back/forward/reload,
  bookmarks-free but functional browsing of five named mainstream sites.
- B3. A documented example init file redefines `C-x b` to a custom buffer switcher and adds a
  minor mode that auto-reader-modes a given domain; both work after `reload-init` with no
  restart.
- B4. `apropos "webview"` lists every native function touching webviews, with docstrings.
- B5. `git grep` in Rust crates for site names, keybinding strings, or command names returns
  nothing: policy lives in `clj/`.
- B6. Startup to first paint under 1.5 s on a developer laptop in release build (Servo
  dominated; record the number, do not gate on it until Phase 4).
- B7. Automated: a Clojure test suite for keymap resolution, layout, buffer registry, and
  mode precedence runs in CI under the interpreter and under AOT.

### Phase 3: Deep web integration

Goal: web content as programmable data; custom buffer types; security boundaries for scripts.

Deliverables:

1. DOM API (`bulwark.dom`), async, handle-based: `query`, `query-all`, `text`, `attr`,
   `html`, `set-attr!`, `set-text!`, `remove!`, `click!`, `observe` (MutationObserver backed),
   `on` (DOM event subscription). Returns core.async channels or promise-like values; node
   handles invalidated on navigation with a clear error.
2. Page lifecycle hooks: `request_navigation` surfaced as a hook that can deny or redirect;
   `:page/dom-ready`, `:page/load-finished`, `:page/unload`.
3. Content extraction helpers: readable-article extraction, tables → vectors of maps, links,
   metadata (OpenGraph etc.), implemented in Clojure over `bulwark.dom`.
4. Page transformations: user styles and scripts per domain, injected via
   `UserContentManager`; a "reader mode" as the reference transformation.
5. Custom buffer types: Option B from D6, `bulwark://` buffers rendered by Servo from hiccup
   produced in Clojure, with a privileged bridge whose capability set is declared per buffer
   type. Reference: a dashboard buffer aggregating titles from several open web buffers.
6. Security: capability sets per D8, extension loading from a directory with a manifest
   declaring capabilities, denial surfaced as a Clojure exception and a log entry.
7. Resource interception: `load_web_resource` exposed as a hook for blocking and rewriting
   requests (the basis for ad/tracker blocking written in Clojure).

Acceptance criteria:

- C1. A Clojure-only extension of under 200 lines implements "Hacker News front page as an
  EDN table in a dashboard buffer, refreshed every 5 minutes, with keybindings to open
  stories in new web buffers."
- C2. A reader-mode command works on five mainstream article sites.
- C3. An extension declaring no `:net` capability cannot navigate or intercept; the denial is
  enforced in Rust (demonstrated by a test that bypasses the Clojure wrapper and calls the
  native function directly).
- C4. DOM handles from a page are rejected after that page navigates; test exists.
- C5. The `bulwark://` bridge rejects messages from non-privileged origins; test exists.
- C6. p50 latency of `(dom/text (dom/query "h1"))` under 10 ms on a simple page.

### Phase 4: Daily-driver viability

Goal: ordinary browsing is reliable enough to use all day, within Servo's limits.

Deliverables:

1. Profiles: profile directory, multiple profiles, `--profile` flag.
2. History, bookmarks, sessions: persistent, searchable, restorable after crash; storage per
   the D7 decision, writes off the shell thread.
3. Downloads with progress buffer; authentication prompts (`request_authentication`); site
   permissions (`request_permission`) with a persisted allow/deny table; cookies and storage
   via Servo's `SiteDataManager`; form controls and dialogs via `show_embedder_control`.
4. Crash containment: `notify_crashed` turns a web buffer into an error buffer with a reload
   command; the shell survives.
5. Compatibility matrix: a maintained list of 30 sites with pass/partial/fail and the Servo
   issue link for each failure.
6. Packaging: release builds for Linux (AppImage or tarball) and macOS (app bundle), with
   Servo features chosen deliberately (likely `multiprocess`, `js_jit`, `devtools`,
   `media-gstreamer`).
7. Performance and recovery: memory ceiling behaviour for many buffers, session autosave
   interval, "kill all web buffers and restore" command.

Acceptance criteria:

- D1. One developer uses Bulwark as their primary browser for two consecutive weeks and
  records blockers; fewer than three are Bulwark's rather than Servo's.
- D2. Kill -9 during browsing; relaunch restores buffers, windows, and history.
- D3. Login flows (cookie-based session) persist across restart on three sites.
- D4. The compatibility matrix is published in `docs/compat.md` and linked from the README.
- D5. A clean machine can install from the release artifact and reach first paint without
  building from source.

### Phase 5: Advanced programmability

Goal: a platform rather than a browser. These are directions, each becoming its own
mini-plan when picked up; none is scheduled.

- Reactive buffers: a buffer whose content is a function of app state and external data,
  re-rendered on change (builds on `bulwark://` buffers and core.async).
- Snapshots and replay: serialize app state (not Servo state) per the "persistent,
  inspectable state" principle; replay command sequences against a fresh shell.
- Background tasks: worker isolates from `cljrs-async` for scheduled scraping and
  aggregation, with capability sets.
- Agent and remote control: a structured external API (nREPL already exists; add a
  JSON/EDN-over-socket API with the same capability enforcement) so an external agent can
  drive the browser within declared limits.
- Packages: a manifest format, a registry, `install-package`, upgrade, and the capability
  review UI.

Acceptance criterion for the phase as a whole: at least one application that is "not a
browser" (a dashboard, a research tool, a feed reader) is built on Bulwark by someone other
than its author, without changes to Rust.

---

## 7. Cross-cutting practices

- **Platform priority.** macOS is the first development and validation platform, per Q2.
  Linux remains follow-up work. Its runtime CI coverage stays in place.
- **Tests.** Rust unit tests per crate; Clojure tests in `clj/` runnable under both
  interpreter and AOT; one headless integration test per native function using
  `SoftwareRenderingContext` and local HTML fixtures. Runtime CI covers macOS and Linux from
  bootstrap. Native Servo validation starts on macOS.
- **Docs.** Every native function and every public `bulwark.*` function has a docstring,
  because `apropos` and `describe-*` are features. `docs/adr/` for decisions. A
  `docs/native-api.md` generated from docstrings at build time.
- **Upstream engagement.** Keep a `docs/upstream.md` listing Servo and Clojurust issues
  Bulwark depends on, with status. Contribute fixes rather than working around them when the
  fix is small.
- **Performance budget.** Record startup time, memory with 10 web buffers, and DOM round-trip
  latency at each phase exit; regressions are discussed, not silently accepted.
- **Scope discipline.** Per the vision's risk table: a small programmable vertical slice
  before comprehensive features. Anything not needed for the current phase's acceptance
  criteria is a note in §8, not code.

---

## 8. Open questions for the project owner

- **Q1. Resolved 2026-10-27.** Corium is not part of this project; see §2.3.
- **Q2. Resolved 2026-10-27: macOS first.** The owner selected macOS as the first platform.
  Initial native validation gates macOS. Linux remains follow-up work with runtime CI coverage.
  The owner has not set a Windows schedule.
- **Q3. Resolved 2026-10-27: use egui.** The owner accepted D6 Option A for the initial chrome.
  The echo area, modeline, minibuffer, and REPL use egui in Phases 1 and 2.
  Privileged Servo buffers remain conditional Phase 3 work. See [D6 ADR](adr/0006-chrome-rendering.md).
- **Q4. Resolved 2026-10-27: prepare upstream fixes.** Required Clojurust fixes are in scope.
  Prepare changes in `csm/clojurust` with regression coverage and retain D10's contribution policy.
  E2 needs no upstream change. Bounded nREPL polling and interruption remain candidates for E6.
- **Q5. Resolved 2026-10-27: single process first, then compare multiprocess.**
  Complete E3 with the current single-process configuration.
  Compare multiprocess behavior and costs after that baseline works.
  Use the results to decide whether to change the default.
- **Q6. Extension language surface.** Are extensions `.cljrs` only, or should `.cljc` with
  reader conditionals be supported for sharing code with JVM/CLJS tooling?
- **Q7. Direction for investigation, 2026-10-27.** The owner requested pure async Clojurust user code with Servo and Clojurust on separate threads.
  E7 implements and measures this direction. D1 now records the completed ownership review and the Phase 1 queue contract.

---

## 9. Immediate next actions

Phase 0 is complete. The [exit review](phase-0-exit.md) records evidence and accepted fallbacks.

1. Promote E3 and E7 mechanisms into the product crates under the D1–D4 contracts.
   Preserve main-thread engine ownership, dedicated runtime ownership, and owned messages across the bridge.
2. Implement Phase 1 native API v0 and event delivery to `bulwark.core/dispatch!`.
   Apply D1 queue, payload, overload, cancellation, and shutdown limits during promotion.
3. Add egui chrome under D6, then Clojure commands, keymaps, hooks, and one end-to-end browser behavior.
4. Add opt-in loopback nREPL with budgeted admission and accepted-work limits.
   Preserve the overload, interruption, recovery, and shutdown regressions from E7.
5. Run the Phase 1 acceptance checks against the product executable.
   The current bootstrap does not implement the native browser API or satisfy those criteria.

E5 remains mandatory for simultaneous web content splits.
Linux native validation, multiprocess comparison, IME, clipboard, and display-scale transitions remain recorded follow-up work.
The [gap register](clojurust-async-gaps.md) records the adopted upstream mechanisms and their limits.

---

## 10. Bootstrap progress (2026-10-27)

Implemented:

- Four workspace crates per D9, plus standalone E1, E2, and E3 programs.
- Embedded `bulwark.core`, a runtime source evaluator, and a host callback bridge.
- Exact upstream pins and a checked-in lockfile.
- E2 event ordering, callback errors, redefinition, and GC-retention checks.
- E1 headless/native drivers, a concurrent nREPL client, and timing counters.
- E3 window rendering, input forwarding, and bounded screenshot capture.
- E1 native client automation, explicit deadline counters, and a dedicated-thread comparison.
- E6 mid-form interruption, session recovery, async recovery, and blocking-work probes, with headless CI coverage.
- A provisional [scheduling policy](scheduling.md) and E3 clean/debug and release build measurements.
- E7's bounded request/reply/event bridge, pure async user example, real Servo backend, and process-bounded diagnostics.
- Six E7 tests for transport types, request and event overload, disconnect, deadlines, and pending-evaluation invalidation.
- A local HTML input fixture, development instructions, and a runtime CI workflow.
- Owner decisions Q2–Q5: macOS first, egui for initial chrome, preparation of upstream Clojurust
  fixes, and a single-process Servo baseline followed by a multiprocess comparison.

Measured locally on macOS arm64, Rust 1.97.1, debug builds:

| Experiment | Evidence | Status |
|---|---|---|
| E2 tree-walk | 10,000 calls, 200 collections, live objects 5,226 throughout warmup samples | Passed |
| E2 tiered, no JIT backend | 10,000 calls, 200 collections, live objects 9,933 throughout warmup samples | Passed |
| E1 headless, 60 s | 3,432 ticks, 541 async ticks, 463 nREPL evaluations, 112 gaps over 25 ms | Progress established, timing criterion not met |
| E1 headless repeat, 60 s | 3,600 ticks, 545 async ticks, 498 nREPL evaluations, no gaps over 25 ms | Headless check passed, full native criterion pending |
| E1 native, 5 s | 291 redraws, 44 async ticks, no nREPL client | Native loop runs, full criterion pending |
| E1 synchronous stall | Three successful evaluations, maximum gap 2.023 s, driver recovered | Headless stall/recovery demonstrated |
| E1 native repeat, 60 s, after initial redraw | 3,598 redraw callbacks, 544 async ticks, 496 evaluations, one skipped deadline, maximum gap 46.597 ms | Strict timing assertion failed |
| E6 inline, 8 s | Active-form interrupt 0.307 ms, session recovery, maximum poll 237.554 ms, maximum LocalSet slice 202.183 ms | Interruption passed, cooperative limits demonstrated |
| E6 worker, 8 s, before startup separation | Maximum runtime gap 289.017 ms, maximum native gap 50.211 ms | Runtime stalls isolated from equivalent redraw stalls |
| E3 macOS | Native fixture input, scrolling, editing, resize, close, and website navigation passed on 2026-11-06 | Initial macOS milestone passed; Linux pending |
| E4 DOM channel | Trivial p50 49.292 µs; 5,000-element subtree p50 24.741 ms; intercepted message and reply asserted | Passed; D5 selects JavaScript callbacks |
| E3 builds | Clean debug target 2m 30s, first release build 194.83 s, release executable 131,904,848 bytes | Build measurements recorded, release fixture screenshot passed |
| E7 Servo | 119 accepted requests, 62.5 microseconds median transport duration, six painted frames during a 200 ms runtime stall | Initial async bridge mechanism passed |
| E7 async limits | One-second idle wait used about one CPU-second; await under keyword lookup blocked until process termination | Upstream gaps reproduced |
| E1 fibers, Linux headless, 60 s | 3,596 ticks, 538 async ticks, 497 nREPL evaluations, 2 gaps over 25 ms | Headless check passed on Clojurust 0.2.34 |
| E6 fibers, Linux release | Parked waits and offloads keep frames under 25 ms; parked nREPL evaluation ignores interrupt | Policy recorded; interrupt criterion open (csm/clojurust#446) |
| E7 mock, Linux debug, Clojurust 0.2.34 | Bridge assertions pass; idle wait still used about one CPU-second; keyword-await probe completed | CLJ-ASYNC-001 remains; CLJ-ASYNC-002 no longer reproduces |

The initial E1 run overlapped Servo compilation. The repeat ran after the main build.
The Mac was locked during initial UI inspection, and a later inspection timed out.
The 2026-11-06 continuation completed native fixture input and website link navigation checks.
The [native report](native-macos.md) replaces the earlier pending status for the macOS baseline.
The later timing runs separate startup from scheduled redraws and run after compilation.
The [scheduling report](scheduling.md) records their measurement limits and the dedicated-thread comparison.
The CI workflow exists locally. No remote CI result is available yet.
Local formatting, workspace Clippy, Servo-probe Clippy, and Python syntax checks passed.
The workspace test command passed, including two E1 schedule tests and six E7 bridge tests.
The experiment programs provide the current integration assertions.
Full instructions and measurement limits live in [experiments/README.md](../experiments/README.md).
