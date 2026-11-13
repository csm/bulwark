# Phase 2 implementation plan

Status: in progress with owner-confirmed scope, 2026-11-11.
Phase 0 and Phase 1 are complete and accepted by the owner.
This document expands Phase 2 of the [development plan](development-plan.md#phase-2-programmable-browser-shell).
The [vision](bulwark.md) remains authoritative.

## Baseline and scope

The accepted baseline is `codex/phase-1-acceptance` at `e49a8ec`.
Its `docs/phase-1-acceptance.md` records passing A1–A8 and accepts the Rust/Clojure boundary.
Implementation uses `codex/phase-2`, which starts from that accepted baseline.
The [mechanism checkpoint](phase-2-mechanisms.md) records retained webviews, the AOT path, and the upstream compiler fix.
The [buffer checkpoint](phase-2-buffers.md) records P2.2.
The [window checkpoint](phase-2-windows.md) records P2.3.
The [mode checkpoint](phase-2-modes.md) records P2.4.
The [minibuffer checkpoint](phase-2-minibuffer.md) records P2.5.

Phase 2 delivers buffers, layouts, modes, completion, introspection, default browsing commands, user configuration, and an internal REPL.
Commands, hooks, keymaps, title history, input reservations, and opt-in nREPL already exist in the accepted baseline.
Phase 2 extends those interfaces and preserves live command redefinition.

Rust owns native resources, engine integration, input conversion, UI rendering, bounded transport, and shutdown.
Clojure owns application state, layout decisions, commands, completion, modes, default bindings, and browser policy.
Only owned data crosses the runtime/engine boundary.
The separate engine and runtime threads, explicit resource cleanup, and existing overload behavior remain required.

The owner confirmed the following scope choices on 2026-11-11:

| Decision | Confirmed scope | Consequence |
|---|---|---|
| Simultaneous web splits | Keep the accepted single-visible-webview fallback. | Multiple live web buffers remain required. Text and REPL splits use egui. |
| B3 auto-reader example | Use a small Clojure/JavaScript transformation for one documented domain. | General article extraction and the structured DOM API remain Phase 3 work. |
| B2 sites | Wikipedia, Hacker News, GitHub, MDN, and BBC News. | Acceptance uses explicit tasks and records engine limitations. |

Full DOM APIs, privileged HTML buffers, extension capability isolation, session restoration, downloads, and packaging remain later-phase work.
Phase 2 configuration and the internal REPL run as trusted code, like the existing nREPL.
Page data cannot select or run shell commands.

## Implementation order

Each milestone produces a reviewable change with its own evidence.
The order exposes native and runtime gaps before the dependent UI work.
The milestone checks define the evidence required before Phase 2 acceptance.

### P2.1: Prove the required mechanisms

1. Start from the accepted Phase 1 baseline.
2. Record the dependency pins and the Phase 1 checks that subsequent changes must preserve.
3. Prove several live webviews with one visible view, explicit selection, and independent navigation state.
4. Prove hide/show, focus, resize, input routing, and explicit close with the native rendering context.
5. Prove the AOT test path with a small shell namespace and documented command metadata.
6. Inspect the pinned engine API for zoom, new-window requests, and opener relationships.
7. Record the native operations and event changes required by the remaining milestones.

The older adapter rejects a second live view in one rendering context.
Removing that restriction requires evidence, even under the accepted single-visible-view fallback.
Switching buffers must retain each page's document, history, and form state.
A tab implementation that closes and recreates pages does not satisfy this milestone.

E5 remains deferred beyond Phase 2. Simultaneous web splits are outside this phase.
The multiple-live-view check remains required for retained web buffers.

B7 requires actual AOT compilation and execution of shell tests.
Tiered execution does not count as AOT evidence.
The AOT runner must use the same Clojurust revision as the product.
The current sibling Clojurust checkout cannot substitute for that pin without an explicit dependency change.
Any required upstream fix needs a minimal reproduction, a pinned revision, and a product regression check.

Exit evidence: native view-switching probe, AOT command and result, proposed API inventory, and an updated risk list.

### P2.2: Add buffers and explicit lifecycles

1. Add `bulwark.buffer` with a registry and the `:web`, `:text`, and `:repl` buffer kinds.
2. Define buffer-local variables, major mode, ordered minor modes, and documented buffer metadata.
3. Associate web buffers with explicit native webview handles.
4. Replace the single `bulwark.browser/active-view` assumption with lookup through the selected window and buffer.
5. Preserve the accepted browser helpers as wrappers around that lookup.
6. Add buffer create, select, and kill commands plus lifecycle hooks.
7. Route background page events to their owning buffer.

Buffer records remain ordinary Clojure data.
Resource identifiers obey D3's process-wide, non-reused identity contract.
Logical buffer state and native webview handles remain distinct.
Pure registry transitions accept supplied identifiers so interpreter and AOT tests require no browser engine.

Creation publishes a buffer only after native creation succeeds.
Failure cleanup closes any native resource that no buffer owns.
Kill invalidates references and closes the resource exactly once.
Late events cannot recreate a killed buffer or change another buffer's title.
The default policy for closing the last web buffer lives in Clojure.

Exit evidence: create/select/kill checks, stale-handle checks, background title updates, and retained state across buffer switches.

### P2.3: Add windows and the data-driven UI boundary

1. Add `bulwark.window` with leaf windows and split nodes as immutable data.
2. Add a pure layout function that produces rectangles from the tree and the available viewport.
3. Define selection, split, delete, resize, and buffer-assignment operations.
4. Add bounded UI descriptions for the modeline, echo area, text panels, and editable controls.
5. Add native mechanisms to apply explicit rectangles, visibility, focus, and UI descriptions.
6. Return UI interactions as data for Clojure dispatch.

The initial scope uses one native application frame.
The confirmed layout scope permits a web pane alongside egui text or REPL panes, with at most one visible webview.
A layout that requires a second visible webview returns a clear error under that fallback.
Clojure selects the layout and fallback policy. Rust checks geometry and resource validity.

The UI contract defines logical coordinates, display scale, clipping, minimum sizes, and focus ownership.
Layout revisions identify obsolete UI events and prevent input from targeting a replacement pane.
All descriptions have explicit size limits and preserve D1's queue and shutdown contracts.
Rust renders supported widgets without command names, default bindings, or site rules.

Exit evidence: layout checks, mixed-pane screenshots, pointer routing, focus changes, resize, and hidden-view isolation.

### P2.4: Add modes and contextual keymaps

1. Add `define-major-mode` and `define-minor-mode` with documentation, local variables, and lifecycle hooks.
2. Define keymap precedence as minibuffer context, minor modes, major mode, then the global map.
3. Define minor-mode ordering and prefix conflicts explicitly.
4. Compute one effective keymap for the active input context in Clojure.
5. Publish its reservations through the existing native snapshot mechanism.
6. Reset pending prefixes after buffer, window, mode, or input-context changes.

Proposed minor-mode ordering uses the most recently enabled mode first.
Publication failure retains the previous effective map and native reservation snapshot.
Switching context must keep those two representations consistent.
Reserved strokes never reach the page, including during runtime stalls.
Unbound page keys retain direct native delivery.
Held-key ownership remains stable until release.

Exit evidence: precedence tables, prefix conflicts, live redefinition, context changes, failed publication, and the accepted stall regression.

### P2.5: Add the minibuffer and command palette

1. Add a minibuffer state machine with prompt, input, candidate list, selection, accept, and cancel states.
2. Add deterministic fuzzy completion over the command registry.
3. Show command docstrings and resolve command Vars at invocation time.
4. Add completion providers for buffers, retained history, and URLs.
5. Add address entry, buffer switching, and history selection through those providers.
6. Restore focus after acceptance or cancellation.

Completion and ranking remain Clojure functions.
Rust provides text editing, selection, composition, and focus mechanisms.
Provider revisions prevent obsolete results from replacing a newer query.
The URL provider accepts literal URLs and completes retained URLs without a remote suggestion service.
Search-engine defaults remain a separate product decision.

The ordered event dispatcher must not await an entire interactive prompt.
It records prompt state and continues to process input and engine events.
Only explicit completion resumes the waiting command, with cancellation as a normal result.

Exit evidence: live `M-x`, docstrings, Unicode input, completion, cancellation, focus restoration, and progress during an open prompt.

### P2.6: Add introspection, messages, and configuration

1. Add `describe-command`, `describe-key`, `describe-variable`, and `apropos` as Clojure commands.
2. Display their results in read-only `:text` buffers.
3. Add docstrings, signatures, and resource tags to every native function.
4. Make `apropos "webview"` include every native operation that acts on webviews.
5. Add a bounded `*Messages*` buffer for command, hook, configuration, and evaluation errors.
6. Load the init file after the default configuration and before ordinary user interaction.
7. Add `reload-init` and a documented example init file.

The default init path is `~/.bulwark/init.cljrs`.
The existing profile selection must work without nREPL so configuration does not depend on an open listener.
The proposed profile rule uses `<profile>/init.cljrs` when an explicit profile is selected.
A missing init file keeps the defaults. A configuration error records source location and leaves the browser usable.

Reload is incremental, not transactional rollback of arbitrary trusted code.
Named hooks, commands, bindings, and modes support replacement so repeated reloads do not duplicate registrations.
The example rebinds `C-x b` and enables the domain-specific reader transformation required by B3.
Any EDN files use atomic replacement under the profile directory.
Full history and session persistence remain Phase 4 work.

Exit evidence: complete native introspection, missing and broken init cases, repeated reload, and B3 without restart.

### P2.7: Complete default browsing and the internal REPL

1. Move default policy into `clj/bulwark/default/` through the same public APIs available to init files.
2. Add new buffer, close buffer, switch buffer, back, forward, reload, and address commands.
3. Add find-in-page through bounded JavaScript requests and zoom through a native engine mechanism.
4. Add open-link-in-new-buffer and basic `window.open` handling.
5. Add navigation, load, focus, buffer create/kill, and window-change hooks.
6. Add an egui `:repl` buffer with evaluation, results, errors, input history, and interruption.

The native new-window callback cannot block on a Clojure decision.
P2.1 must establish whether the pinned callback supports deferred completion.
If it does, Clojure handles the request before native creation completes.
Otherwise, record a compatible mechanism and its opener semantics before implementation.
A URL-only replacement must document lost opener behavior instead of claiming full `window.open` compatibility.
Popup policy and foreground/background selection belong in Clojure.
Link targeting uses a bounded page query and treats its result as untrusted data.

The internal REPL shares the existing runtime and globals.
Its evaluation jobs use bounded admission and do not occupy the ordered event dispatcher while parked.
Its output has a documented retention limit.
Closing a REPL buffer cancels its owned jobs without closing nREPL or another buffer's jobs.
The nREPL listener remains opt-in.

Exit evidence: no-init browsing workflow, find and zoom, link opening, popup fixture, hook order, and live changes from both REPL interfaces.

### P2.8: Run acceptance and record the exit review

1. Run the acceptance matrix below from a clean profile.
2. Record release startup measurements on the development laptop.
3. Record manual macOS results and Linux CI results separately.
4. Review every new native function for mechanism-only behavior.
5. Record remaining engine limitations and the disposition of E5.
6. Update the README, development plan, handoff, and Phase 2 acceptance report.

Phase 2 is complete only after the required criteria pass and the owner accepts the exit review.
An unresolved AOT path or a required browsing task does not count as a passing criterion.

## Acceptance matrix

| Criterion | Required evidence |
|---|---|
| B1: preserve Phase 1 | Existing A1–A8 checks pass, including input capture, live redefinition, nREPL, stalls, and bounded shutdown. |
| B2: default browser | A clean profile supports address entry, multiple retained buffers, back/forward/reload, and the five agreed site tasks. |
| B3: live configuration | Reloaded init changes `C-x b` and enables the domain-specific reader mode without restart or duplicate hooks. |
| B4: introspection | The installed native inventory matches documented `apropos "webview"` results. Help opens in text buffers. |
| B5: policy boundary | Rust contains no default bindings, site rules, or browser command policy. A source review records any search false positives. |
| B6: startup | A release build records process-start-to-first-paint samples, hardware, profile state, and the median. The 1.5-second target remains informational. |
| B7: language coverage | The same keymap, layout, buffer, and mode assertions pass under the interpreter and an actual AOT binary in CI. |
| Deliverables beyond B1–B7 | Native checks cover mixed layouts, minibuffer focus, internal REPL, init errors, new-window requests, and lifecycle cleanup. |

B5 searches distinguish native API identifiers and embedded source paths from browser policy.
A literal search cannot require the absence of names such as `reload` that also identify accepted native mechanisms.
The exit review records the search terms, matches, and classification.

B2 tasks for the confirmed site set:

| Site | Task |
|---|---|
| Wikipedia | Open an article, follow an internal link, then use back and forward. |
| Hacker News | Open a story in another buffer, then return to the retained listing. |
| GitHub | Read a public repository and open a source file without authentication. |
| MDN | Open a reference article, find text, and change zoom. |
| BBC News | Open an article, scroll, and reload. |

These tasks define browser acceptance, not present claims about Servo compatibility.
Deterministic local fixtures cover links, history, forms, popups, page events, and reader transformation in CI.
Live-site checks record date, URL, task, and result.
An engine incompatibility needs a recorded scope decision before a failed task can change.

## Review and risk handling

The first implementation review settles the UI schema, buffer identities, contextual keymap publication, and new-window callback contract.
Each API addition documents arguments, replies, events, limits, cancellation, and resource ownership.
Each milestone adds checks for its new behavior and preserves relevant accepted regressions.
Pure Clojure checks remain independent of graphics and live sites.
Native checks cover input, rendering, resource cleanup, and runtime/engine interactions that pure checks cannot establish.

The main early risks are retained hidden webviews, pinned AOT support, contextual input ownership, and engine callback constraints.
P2.1 and P2.3 resolve those risks before the default shell depends on them.
IME, clipboard, display-scale transitions, and interactive Linux behavior retain their existing evidence limits until explicit checks establish support.
