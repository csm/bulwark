# Phase 2 window and UI checkpoint

Status: implementation complete, 2026-11-12. Physical input and current Linux CI evidence remain open.
Base: `main` at `c1fef4d`, followed by the inherited WIP commit `9b0eb20`.

This checkpoint implements the six P2.3 items in the [implementation plan](phase-2-plan.md).
The inherited WIP adds registry integration, UI descriptions, native rendering, and initial regression suites.
The continuation fixes pointer ownership, description limits, deep tree traversal, and Unicode counts in AOT code.
P2.3 remains open until its physical input and Linux checks pass.

## Window tree

`bulwark.window` represents a layout as immutable data:

```clojure
{:root {:split/direction :row :split/share 500
        :split/children [{:window/id 1 :window/buffer 10}
                         {:window/id 2 :window/buffer 11}]}
 :selected 1
 :revision 2}
```

A `:row` split places its children left to right. A `:column` split places them top to bottom.
`:split/share` gives the first child's part of the extent in thousandths.
Integer shares keep layout arithmetic identical in the interpreter, Tiered mode, and the AOT binary.
Every change increments the layout's `:revision`.
The UI boundary uses a separate revision for the complete native frame structure.

| Operation | Behavior |
|---|---|
| `(initial id buffer)` | One selected window. |
| `(split state id new-id direction [buffer])` | The existing window comes first and keeps the selection. The new window shows the same buffer by default. |
| `(delete state id)` | The sibling takes the freed space. A deleted selected window passes the selection to the sibling's first window. The only window cannot be deleted. |
| `(select state id)` | Selects a live window. |
| `(set-buffer state id buffer)` | Shows a buffer in a window. |
| `(resize state id direction amount)` | Grows the window within its nearest enclosing split of that direction. Shares stay between 100 and 900. |

Operations take supplied identifiers. A stale id raises `:stale-window`.

## Layout

`(layout state viewport)` returns one rectangle per window for a viewport `{:x :y :width :height}`.
Coordinates are integer logical pixels. The rectangles tile the viewport exactly. A split's second child takes the remainder.
Each rectangle carries `:window/id`, `:window/buffer`, and the layout's `:revision`.
A window below 80 by 40 logical pixels raises `:window-too-small`.

`(check-web-limit state web?)` enforces the confirmed single-visible-webview fallback.
It raises `:multiple-visible-webviews` when more than one window shows a web buffer.
Text and REPL panes may sit beside one web pane.

## Registry and default commands

`bulwark.buffer` holds the layout beside its buffer registry.
The selected window determines the selected buffer and the browser helper's active view.
A text window can own keyboard focus while a page remains visible beside it.
Selection of another web buffer replaces the page in its existing window.
A layout that shows two web buffers raises `:multiple-visible-webviews` before publication.

A killed buffer's windows take the most recent compatible buffer.
A window without a replacement disappears. The final removal leaves the echo area alone.
New windows use the same process-wide identifier allocator as buffers and webviews.
Failed selection and layout publications retain the previous registry state.

The default split of a web window uses a recent text or REPL buffer, or creates `*scratch*`.
These commands and bindings live in Clojure:

| Binding | Command |
|---|---|
| `C-x 2`, `C-x 3` | Split below or right |
| `C-x 0`, `C-x 1` | Delete this window or keep only this window |
| `C-x o` | Select the next window |
| `C-x ^` | Enlarge vertically |
| `C-x }`, `C-x {` | Enlarge or shrink horizontally |

Tree traversal uses explicit pending lists and ancestor paths.
A 64-leaf nested tree exercises selection, assignment, resize, deletion, and minimum-size rejection without recursive evaluator calls.
The earlier recursive implementation overflowed the runtime thread's native stack with eight nested windows.

## UI boundary

`bulwark.ui` computes a whole-frame description:

```clojure
{:revision 7 :focus 2
 :elements [{:ui/kind :text :ui/id 1 :x 0 :y 0 :width 500 :height 710 :text "Notes"}
            {:ui/kind :web :ui/id 2 :x 500 :y 0 :width 500 :height 710 :webview 10}
            {:ui/kind :echo :x 0 :y 732 :width 1000 :height 28}]}
```

Supported elements are `:web`, scrollable `:text`, `:line`, `:echo`, and single-line editable `:input`.
Modelines use `:line`. `buffer/modeline-format` supplies their Clojure formatter.
`bulwark.native/set-ui!` returns the standard awaitable reply.
`bulwark.native/ui-viewport` returns `{:width :height :scale}`, or nil without a native window.

Coordinates use integer logical pixels from the frame's top-left corner.
The host rounds shared edges once at the display scale and clips the page to the physical window.
Clojure reserves 28 logical pixels for the echo area and 22 for each modeline.
A window below 80 by 40 logical pixels fails a layout change.
After a native resize, an existing layout that no longer fits shows only its selected window.
The registry retains the complete tree, so a larger frame restores its splits.

The description specifies keyboard focus by element id.
The native host forwards free keys to the focused page or editable control.
The visible page receives pointer input in its rectangle, even when a text window owns keyboard focus.
A focus change preserves a pending mouse release for that same page.
A webview change clears ownership, so a replacement page receives no previous page's release.

The host draws web content below the other elements.
The description's hit test determines pointer ownership and respects the other elements' draw order.
An editable control retains user edits while descriptions repeat its prior text.
A changed described value replaces those edits. An absent control loses its edit state.

| Limit | Native behavior |
|---|---|
| 64 elements | Reject a larger description |
| Coordinates and extents through 32768 | Reject invalid or zero-size rectangles |
| One web element | Reject a second web element |
| Positive, unique element ids | Require ids for web, text, and input elements |
| 1024 UTF-8 bytes per line | Reject excess text |
| 4096 UTF-8 bytes per input | Reject excess text |
| 256 KiB of text per description | Reject excess combined text |

Clojure clips each modeline to 256 characters and each text pane to at most 16384 characters.
It also shares a 65536-character budget across the frame, with space reserved for modelines.
Four UTF-8 bytes per character keep every generated description within the native byte limit.
A frame with too many elements shows its selected window alone.

UI requests use the existing bounded request queue, timeout, and shutdown contract.
Viewport changes replace one snapshot and coalesce until Clojure reads it.
A native resize therefore cannot fill the event queue during a runtime stall.

## Revisions and interactions

A UI revision identifies element ids, kinds, rectangles, webview handles, and focus.
Text and emphasis changes keep the revision. A structural change takes the next revision.
Unchanged descriptions send no request.
Rust rejects an older revision, a structural replacement at the same revision, or a stale webview handle before any native change.

Interactions return as `:ui/pressed`, `:ui/input-changed`, or `:ui/input-submitted` events.
Each includes `:ui/id` and `:revision`. Input events also include `:text`.
Frame events contain no `:webview` field.
The Clojure dispatcher discards obsolete interactions before hooks run.
The default press hook selects a live window. Rust contains no window command or binding policy.

## Unicode compiler fix

The Unicode budget regression exposed an upstream AOT defect.
Compiled `(count "😀")` returned 4 bytes, while the interpreter returned one character.
This also made short Unicode strings fail `subs` during clipping.

Bulwark now pins [`174557b3`](https://github.com/csm/clojurust/commit/174557b3eec18117f0b4886076961fe3223316c0), one commit after the previous pin.
[Draft PR #491](https://github.com/csm/clojurust/pull/491) targets the existing AOT fix branch.
It changes `rt_count` to count Unicode scalar values, as the interpreter does.
The ABI regression failed before the fix. Three count ABI tests and an actual AOT executable pass after it.
The product regression describes eight Unicode panes and exercises the native byte limit.
No local dependency override supplies these results.

## Checks and remaining evidence

The local macOS checks cover these areas:

| Check | Evidence |
|---|---|
| Portable workspace | 50 tests, including the model and mock bridge suites in both evaluator modes |
| Native unit checks | 17 tests, including egui editing, clipping, scaled geometry, pointer ownership, and hidden-view notifications |
| Servo contract | Real retained views, UI revisions, rejected frames, visibility, focus, scale, resource cleanup, and shutdown |
| Native processes | Ten cases per evaluator mode, including the new mixed-window fixture |
| Native nREPL | Four cases per evaluator mode, including overload, interruption, live commands, and listener cleanup |
| Mock nREPL | Three cases per evaluator mode, including overload, live code, and listener cleanup |
| Software Servo | Both evaluator modes, retained documents, and joined workers |
| AOT | Six compiled shell namespaces and the same portable model assertions |
| Formatting and Clippy | Portable workspace and native targets |

`clj/tests/windows-native.cljrs` exercises real mixed panes, resize, a page at a nonzero origin, text focus descriptions, and retained hidden documents.
It also covers a text-only frame and removal of a hidden web buffer.
The composed screenshot shows text on the left, Servo on the right, two modelines, and the echo area.
The screenshot establishes composition, not physical input delivery.

The native runner can retain screenshots:

```sh
python3 experiments/run_product_native.py --artifacts-dir target/window-evidence
python3 experiments/run_product_native.py --interpreter --artifacts-dir target/window-evidence
```

The files use `page-and-chrome-<mode>.png` and `mixed-windows-<mode>.png`.
This continuation retains both modes under `target/window-evidence/`.
The runner joins the runtime in every case, including expected failures.
The existing GL texture diagnostic and large `__eh_frame` linker warning remain.

Computer Use reported a locked Mac during this continuation.
Physical pointer routing, held mouse release during window selection, keyboard focus changes, and interactive resize remain open.
The automated checks do not establish physical IME, clipboard, display-scale transitions, or Linux input behavior.
Current Linux CI remains open. The workflow already includes the extended portable, native process, and AOT suites.

After these P2.3 checks pass, the next milestone is P2.4: modes and contextual keymaps.
