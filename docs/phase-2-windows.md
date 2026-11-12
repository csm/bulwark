# Phase 2 window model checkpoint

Status: portable model implemented, 2026-11-12. P2.3 remains in progress.
Branch: `claude/cool-faraday-q2ev46`, after the [buffer checkpoint](phase-2-buffers.md).

This checkpoint implements items 1–3 of P2.3 in the [implementation plan](phase-2-plan.md) as pure data.
It does not connect the layout to the registry, the native surface, or egui.
Items 4–6, the bounded UI descriptions and native mechanisms, remain open.

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
Every change increments `:revision`. P2.3's UI events will carry it, so events from an older layout can be discarded.

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
Coordinates are integer logical pixels. The rectangles tile the viewport exactly; a split's second child takes the remainder.
Each rectangle carries `:window/id`, `:window/buffer`, and the layout's `:revision`.
A window below 80 by 40 logical pixels raises `:window-too-small`.

`(check-web-limit state web?)` enforces the confirmed single-visible-webview fallback.
It raises `:multiple-visible-webviews` when more than one window shows a web buffer.
Text and REPL panes may sit beside one web pane.

## Checks and remaining work

`clj/tests/shell-model.cljrs` covers tiling, the viewport origin, split order, resize, clamping, delete, selection, stale ids, the minimum size, and the web limit.
It passed in the interpreter, Tiered mode, and the standalone AOT binary, which now compiles five shell namespaces.
Formatting, portable Clippy, and all 43 portable workspace tests passed on Linux on 2026-11-12.

The remaining P2.3 work:

1. Hold the layout in a registry and make buffer selection act on the selected window.
2. Replace or delete windows whose buffer is killed.
3. Define bounded UI descriptions for the modeline, echo area, text panels, and editable controls.
4. Add native mechanisms for rectangles, visibility, focus, and UI descriptions.
5. Return UI interactions as revision-tagged data for Clojure dispatch.
6. Record mixed-pane screenshots, pointer routing, focus changes, resize, and hidden-view isolation.

Items 3–6 change the native shell. They need a Servo build for validation.
