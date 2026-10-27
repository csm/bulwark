# Bulwark Development Plan

Status: draft v0.1 (2026-10-27). Derived from `docs/bulwark.md`, Section 9 ("Next Steps"),
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
- **Calling Clojure from Rust is not a documented public API.** The interop example only goes
  Clojure → Rust. `cljrs_runtime::interp::apply` exists as a module, and `TODO.md` mentions
  internal re-entry via a thread-local context. Event delivery from Servo into Clojure
  callbacks depends on this; it is Experiment E2.
- Threading: `GcPtr`, `Value`, `Env`, and `GlobalEnv` are **not `Send`**. The runtime lives on
  one thread. `cljrs-async` runs core.async (`go`, `chan`, `<!`, `>!`, `alts!`, `timeout`) on
  a Tokio `current_thread` runtime with a `LocalSet` that the host must drive on that same
  thread; `cljrs_async::init(&globals)` must be called inside that `LocalSet`. The crate
  exposes `spawn_future`/`await_value`, and has `isolate`, `isolate_channel`, and
  `worker_pool` modules whose contents were not verified.
- nREPL: `cljrs_nrepl::start(Config, globals) -> Server` spawns its own network thread but
  evaluates on the calling thread. `Server::into_poller()` yields a `Poller` with
  `poll(&mut eval_form)` / `poll_timeout(..)` for hosts that run their own loop. This is
  exactly the shape Bulwark needs.
- GC caveats from `TODO.md`: finalization hooks, weak references, and write barriers are
  deferred. Native resources held from Clojure (webviews, buffers) therefore cannot rely on
  GC finalizers for cleanup; lifetimes must be explicit, which the vision's risk table already
  prescribes ("handle-based APIs with explicit lifetimes and invalidation").
- Other gaps relevant here: `cljrs-tx` (STM/refs) is "initial"; `locking` deferred; LSP is
  syntactic only; the calling convention for Clojure → Rust is listed as still open.

### 2.3 Corium (`csm/corium`)

The vision document says "Clojurust (Corium)". Corium is not an alias for the runtime: it is a
Datomic-style database (immutable, time-aware, fact-oriented, Datalog) written in Rust, with a
`corium-cljrs` crate providing value conversion, a `corium.api` namespace, and a sandbox host
for `:db/fn` functions. Storage backends include in-memory, filesystem, PostgreSQL, Turso, S3.

Reading: the vision intends Corium as a candidate for the "persistent, inspectable state"
principle (history, bookmarks, sessions, snapshots, replay). This plan treats it that way
(decision D7) and flags the phrasing as an open question (§8, Q1).

### 2.4 Implications that drive the architecture

1. Servo and Clojurust are both single-thread-affine. The natural design is **one shell
   thread** that owns the `winit` event loop, the `Servo` instance, the Clojurust `Runtime`,
   the Tokio `LocalSet`, and the nREPL `Poller`. Everything else (network, disk, Corium) runs
   on other threads and communicates by message.
2. Because Clojure runs on the UI thread, a slow Clojure function freezes rendering and
   input. Mitigation is architectural (cooperative scheduling via core.async, time-budgeted
   event dispatch, worker isolates for heavy work), not optional.
3. DOM access must be built by Bulwark, as injected JS plus a message protocol, because
   Servo exposes only `evaluate_javascript` and `UserContentManager`.
4. Native-handle lifetime management is Bulwark's responsibility: handles are revocable
   tokens, not GC-managed objects.
5. Bulwark will exercise undocumented or missing Clojurust paths (Rust → Clojure callbacks,
   host-driven `LocalSet`, interop calling convention). Expect to land changes upstream in
   `csm/clojurust`; budget for it.

---

## 3. Architecture and API decisions required before implementation

Each decision lists the recommended default. Defaults are chosen so that Phase 1 can start;
any can be revisited when an experiment in §5 contradicts it. Record final choices as short
ADRs under `docs/adr/`.

**D1. Threading and event-loop ownership.**
One shell thread hosts `winit`, `Servo`, the Clojurust runtime, the Tokio `LocalSet`, and the
nREPL poller. Servo's `EventLoopWaker` posts a `winit` user event. The loop iteration order is:
drain winit events → forward input to the focused `WebView` → `Servo::spin_event_loop` →
dispatch queued engine events to Clojure → run the `LocalSet` until idle or until a time
budget expires → `Poller::poll_timeout(0)` for nREPL → paint. Default: adopt. Experiment E1
validates it.

**D2. Rust ↔ Clojure boundary is data, not objects.**
Native capabilities are exposed as a small set of namespaced native functions
(`bulwark.native/*`) that take and return plain Clojure data (maps, vectors, keywords,
strings, numbers) plus opaque handle values. Engine events are delivered as event maps
(`{:event/type :page/url-changed :webview 7 :url "https://..."}`) into a single Clojure
dispatcher function; all routing, hooks, and modes are Clojure. Rust never calls arbitrary
Clojure functions, only the registered dispatcher and a registered "idle/tick" entry point.
This limits the Rust → Clojure callback surface to one mechanism (E2) and keeps the native
API small enough to document and stabilize.

**D3. Handles are explicit, revocable tokens.**
A webview, buffer, window, download, or subscription is identified by an integer or keyword
id issued by Rust. Rust owns a registry mapping id → native resource. Clojure holds only ids.
Invalidation is explicit (`(bulwark.native/webview-close id)`), and using a dead id returns a
structured error, never UB. This follows directly from the GC caveats in §2.2.

**D4. Engine abstraction boundary.**
Define a Rust trait `Engine` (or a module boundary) covering: create view, destroy view,
navigate, history, resize/show/hide/focus, input forwarding, evaluate script, inject user
content, intercept resource loads, and an event stream. The Servo implementation is the only
one, but no Servo type crosses into the Clojure-facing API layer. Keep this boundary cheap:
one crate `bulwark-engine-servo` behind `bulwark-engine` traits, no premature generality.

**D5. DOM API strategy.**
Structured DOM access is implemented as (a) a Bulwark-owned JS agent injected via
`UserContentManager` into every page in an isolated-world-like namespace, (b) requests sent
with `evaluate_javascript`, (c) responses and page-originated events returned through the
same evaluate round trip (or a custom `bulwark://` resource load intercepted by
`load_web_resource` as a back-channel, if evaluate proves too slow). Results are serialized
as JSON and converted to EDN at the boundary. Node references are session-scoped handles
registered by the agent, invalidated on navigation. Experiment E4 picks the channel.

**D6. Chrome and non-web buffer rendering.**
Two viable options, with a recommended sequence:

- Option A: `egui` chrome, as servoshell does. Clojure describes the UI as data (hiccup-like
  vectors); Rust renders it with `egui`. Fast, no security concerns, but a second UI model.
- Option B: non-web buffers are privileged Servo webviews loading `bulwark://` URLs served by
  `load_web_resource` interception, with HTML generated from hiccup in Clojure. One rendering
  model, but depends on Servo maturity and needs a hardened privileged bridge.

Default: Option A for the minimal chrome needed in Phases 1 and 2 (echo area, modeline,
minibuffer/command palette, REPL buffer). Option B for user-defined buffer types from Phase 3,
once E5 shows multiple webviews can coexist and the bridge can be made safe. Window splits
and layout are pure Clojure data in both options; Rust only receives rectangles.

**D7. Persistence.**
Phase 1 and 2: EDN files under the profile directory, written atomically. Phase 4 decides
between Corium (strong fit for history/snapshots/replay, same author, `corium-cljrs` binding
exists) and SQLite (boring, proven for browser profiles). Decision criteria: startup cost,
binary size, write latency on the shell thread (must be off-thread either way), and whether
time-travel queries are actually used by then. See Q1.

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
            │ E1 shell thread loop │
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

- Phase 1 cannot start until E1, E2, E3 pass. These three are the architectural hypothesis.
- Phase 2 needs E5 only for split windows showing multiple web pages at once; a Phase 2 with
  one visible webview at a time (Emacs with a single window) is acceptable fallback.
- Phase 3 needs E4.
- Phase 4 needs the D7 decision and Servo compatibility results; it is the phase most exposed
  to upstream Servo limitations and should not be scheduled to a date.

Clojurust capabilities each phase relies on:

| Phase | Clojurust capability | Status |
|---|---|---|
| 1 | embed runtime, native fns, `builtin_source`, nREPL poller | documented |
| 1 | Rust → Clojure dispatcher call | undocumented (E2) |
| 1 | core.async on host-driven `LocalSet` | documented, host integration unverified (E1) |
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

**E2. Rust → Clojure callback.** From Rust, look up a var (e.g. `bulwark.core/dispatch!`) and
invoke it with a map argument, 10,000 times, with the GC active, without leaks or crashes.
Pass: a documented or newly contributed public API in `cljrs-runtime` for `apply` of a
`Value` function from host code, with GC-root safety stated. Time box: 3 days plus upstream
PR time. If it fails: Rust pushes events into a core.async channel instead (verify
`cljrs-async` `channel` module supports host-side puts), and Clojure pulls; this is
acceptable and may even be preferable.

**E3. Servo embed.** Using the `servo` crate, open a `winit` window, create one `WebView`
with `WindowRenderingContext`, load a URL, paint on `notify_new_frame_ready`, forward
keyboard and mouse. Record build time, binary size, and which Servo features are required.
Pass: renders `https://servo.org` and `https://en.wikipedia.org` interactively on Linux and
macOS. Time box: 3 days (mostly build time). If it fails: the project is blocked on upstream;
stop and report.

**E4. DOM channel.** Inject an agent via `UserContentManager`; measure round-trip latency of
`evaluate_javascript` for (a) a trivial expression, (b) serializing a 5,000-node subtree to
JSON; test the `load_web_resource` intercept as an alternative page → host channel. Pass: a
chosen channel with p50 latency under 5 ms for (a) and a documented upper bound for (b).
Time box: 3 days. Runs after E3.

**E5. Multiple visible webviews.** Two `WebView`s visible simultaneously in one window at
different rectangles, both animating. Try: one rendering context with show/hide and resize
per frame; two `OffscreenRenderingContext`s composited by the shell. Pass: two pages
interactive side by side at acceptable frame rate. Time box: 4 days. Runs after E3. If it
fails: Phase 2 ships with one visible web buffer per frame, with non-web buffers in `egui`.

**E6. Clojure-on-UI-thread time budgeting.** Instrument the E1 loop: cap Clojure dispatch
per frame, measure how core.async `go` blocks yield, and test whether a long synchronous
Clojure call can be interrupted (nREPL `interrupt` exists; does it reach the interpreter
mid-form?). Pass: a written policy for what runs where, and an `interrupt` that works from a
second nREPL connection. Time box: 2 days. Runs after E1.

Phase 0 exit: E1, E2, E3 passed; E4, E5, E6 either passed or have a recorded fallback; the
ADRs for D1–D6 written with final choices.

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
3. Event delivery: Rust calls `bulwark.core/dispatch!` with event maps for url-changed,
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
- A7. A 2-second synchronous Clojure loop at the REPL freezes the UI for 2 seconds and then
  recovers, with no crash. (Known limitation, must be documented, must not corrupt state.)
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
  inspectable state" principle; replay command sequences; Corium's time-travel model is the
  obvious fit if D7 chose it.
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

- **Tests.** Rust unit tests per crate; Clojure tests in `clj/` runnable under both
  interpreter and AOT; one headless integration test per native function using
  `SoftwareRenderingContext` and local HTML fixtures. CI on Linux from Phase 1, macOS from
  Phase 2.
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

- **Q1. Corium's role.** Is "Clojurust (Corium)" in the vision meant to name Corium as the
  state store, or is it just a label? This plan assumes the former as a Phase 4 candidate.
- **Q2. Platform priority.** Linux first, macOS second, Windows later? Servo's Windows story
  (`no-wgl`) and GStreamer packaging make Windows the costliest; this plan assumes Linux +
  macOS through Phase 4.
- **Q3. egui as the chrome renderer (D6 Option A).** Acceptable as a dependency, or is the
  intent that even the echo area is a Servo-rendered buffer from day one? The plan defers
  that to Phase 3 for risk reasons.
- **Q4. Clojurust changes.** Confirm that landing changes in `csm/clojurust` (public apply
  API, host-driven LocalSet helpers) is in scope for this project's timeline rather than
  something to work around.
- **Q5. Multiprocess Servo.** `servo/multiprocess` improves crash containment but changes how
  `evaluate_javascript` and interception behave. Decide in E3 whether Phase 1 enables it.
- **Q6. Extension language surface.** Are extensions `.cljrs` only, or should `.cljc` with
  reader conditionals be supported for sharing code with JVM/CLJS tooling?

---

## 9. Immediate next actions

1. Create the workspace skeleton per D9 and an `experiments/` directory. Pin `servo` and
   `csm/clojurust` per D10; record the versions in `docs/upstream.md`.
2. Run E3 first (its build time dominates), E1 and E2 in parallel with it.
3. Write ADRs for D1–D3 as soon as E1/E2 results are in.
4. Run E4, E5, E6; hold the Phase 0 exit review; amend this plan.
5. Begin Phase 1 with deliverables 1–3, in that order, since A1–A3 depend on them.
