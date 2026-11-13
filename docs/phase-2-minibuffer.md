# Phase 2 minibuffer and command palette checkpoint

Status: implemented, 2026-11-13. Linux CI and physical keyboard checks remain open.
Base: `main` at `dcf9a23`, which contains P2.4.

This checkpoint implements the six P2.5 items in the [implementation plan](phase-2-plan.md).
Clojure owns the prompt state, completion, ranking, commands, and bindings.
Rust gains two small mechanisms for the editable control: a read-only label and cursor placement.

## Command tasks

The ordered event dispatcher no longer awaits a whole command.
`bulwark.keymap/dispatch-key!` runs each command as a task through `(commands/run-task! thunk)`.
The dispatcher waits until the task finishes or suspends.

| Operation | Behavior |
|---|---|
| `(commands/run-task! thunk)` | Starts a task. Returns the awaited value, or `:suspended`. An error before suspension reaches the caller. |
| `(commands/suspend!)` | Releases the fiber that waits for the running task. Returns the task, or nil outside a task. |
| `(commands/resume! task wake)` | Calls `wake`, then waits until the task finishes or suspends again. |

`commands/*task*` names the running fiber's task. Fibers that the task starts inherit it.
A prompt's reader suspends before it waits. The dispatcher then continues with input and engine events.
The event that ends the prompt resumes the reader and waits for it.
The rest of the command therefore runs in event order, like a command without a prompt.
An error after a resumption reaches the dispatcher through that event, as other command errors do.
A task that fails while no fiber waits for it runs `:command/failed` hooks with `:error`. The shell shows that error in the echo area.

A cancel stroke now has two results in `:keymap/status`.
`:cancelled` means that the stroke cancelled a pending prefix. `:quit` means that no prefix was pending.

## Prompt state

`bulwark.minibuffer` holds one prompt as immutable data. Its transitions are pure and take a supplied id.

```clojure
{:minibuffer/id 40 :status :reading :prompt "M-x "
 :input "rel" :described "" :candidates [{:text "reload" ...}] :selected 0
 :revision 3 :answered 3 :rows 8 :message nil
 :provider f :literal nil :result nil}
```

| Transition | Behavior |
|---|---|
| `(start id options)` | A new prompt in the `:reading` status. |
| `(edit state text)` | Records the control's text. A changed text takes the next revision and clears the selection. |
| `(answer state revision candidates)` | Applies a provider's candidates. An answer for an older revision changes nothing. |
| `(move state offset)` | Moves the selection and wraps at either end. |
| `(complete state)` | Replaces the input with the selected candidate's text, or the first one's. |
| `(accept state)` | Finishes with the selected candidate, else with the literal input. Otherwise shows `No match` and stays open. |
| `(cancel state)` | Finishes with `{:status :cancelled}`. |
| `(view state)` | The prompt for `bulwark.ui`: an id, a label, a text, and the candidate lines. |

Options are `:prompt`, `:provider`, `:literal`, `:initial`, and `:rows`.
A prompt with a provider and no `:literal` accepts only a candidate, and selects the first one.
`:literal` is a function from the input to a result, or to nil when the input is no result.
A prompt without a provider returns its input.
The input is clipped to 1024 characters. A prompt keeps at most 256 candidates.

## Reading

```clojure
(let [reply (await (minibuffer/read! {:prompt "Buffer: " :provider provider}))]
  (when (= :accepted (:status reply))
    (await (buffer/select! (:value reply)))))
```

`read!` returns `{:status :accepted :value v :input text :candidate c}` or `{:status :cancelled}`.
Cancellation is a normal result, not an error.
`open!` returns the result promise without waiting. `accept!` and `cancel!` finish the open prompt from any fiber.
A reader outside the dispatcher, such as an nREPL evaluation, receives the result directly.

Only one prompt exists. A new prompt first cancels the open one, and its reader sees the cancellation.
A reader that opens another prompt in reply is cancelled again. After eight rounds, `open!` raises `:minibuffer-busy`.

`open!` publishes the `:minibuffer` input context before the prompt exists, then the frame.
A refused key publication changes nothing. A refused frame removes the prompt and the context, then raises.
After acceptance or cancellation, the description's focus returns to the selected window.
A failed context removal keeps the previous reservations. `minibuffer/context-failure` holds the error.

The `:minibuffer` keymap is an ordinary named keymap, so configuration can change it:

| Key | Command |
|---|---|
| `ArrowDown`, `C-n` | `bulwark.minibuffer/next-candidate` |
| `ArrowUp`, `C-p` | `bulwark.minibuffer/previous-candidate` |
| `Tab` | `bulwark.minibuffer/complete-candidate` |

`Enter` stays a free key. The native control reports `:ui/input-submitted` with its final text.
That text is authoritative, so acceptance never depends on a change event that has not arrived.
`C-g` and `Escape` remain cancel strokes. A `:quit` status cancels the open prompt.
Global bindings stay active during a prompt.

## Completion

`bulwark.completion` contains pure functions with integer scores.
A candidate is a map with `:text`, and optionally `:value`, `:detail`, and `:match`. A string is a candidate too.
`(score query text)` needs every query character in order, without regard to case.
A match scores more at the start of the text, after a separator, and next to the previous match.
A text that contains or equals the query scores more. A shorter text scores slightly more.
`(rank query candidates)` returns the matches, best first. Equal scores keep the supplied order.
`(fuzzy source)` returns a provider that ranks the source's candidates for every query.

A provider is a function of the input. It returns candidates, best first, or a future of them.
Each change of the input starts one query on its own fiber, so a slow provider stops no event.
The prompt's revision identifies the query. An answer for an older revision is discarded.
Acceptance first awaits the query for the final text, when that answer is still missing.
A provider error appears as a message before the prompt.

`bulwark.browser` supplies the providers and commands. They remain default policy until P2.7 moves it.

| Binding | Command | Provider | Result |
|---|---|---|---|
| `M-x`, `C-c x` | `execute-command` | Every command, with the first line of its docstring | Runs the command through its current Var |
| `C-x b` | `switch-buffer` | Buffers by recency, with the selected buffer last | Selects the buffer |
| `C-l` | `open-url` | URLs of open pages, then retained URLs | Shows a candidate or a literal URL |
| `C-c h` | `select-history` | Retained visits, newest first, matched by title or URL | Shows the visit |

A command name that two namespaces share stays qualified in the candidate list.
`open-url` selects no candidate at first, so `Enter` takes the typed text.
`literal-url` accepts text with a scheme, gives a host name `https`, and gives `localhost` `http`.
Other text is no URL. No remote service suggests one, and no search engine is configured.
`C-x h` still shows recent titles in the echo area.

## Frame and native mechanisms

`(ui/set-prompt! view)` sets the prompt of the next frame. `ui/describe` takes it as a fourth argument.
The prompt's `:input` element replaces the `:echo` element and owns the focus.
Its candidate lines are `:line` elements directly above the input. They cover the windows.
The number of lines is fixed while a prompt is open. Missing candidates are empty lines.
Typing therefore keeps the frame's structure and its revision, and later interactions stay current.
An interaction that still carries an older revision is discarded, as before.
The description reserves the prompt's elements and text before it divides the budget among panes.
The echo text has no element during a prompt. `set-echo!` still stores it.

The native changes contain no command, binding, or completion policy:

| Mechanism | Behavior |
|---|---|
| `:label` on an `:input` element | A read-only label before the control, at most 1024 UTF-8 bytes. A label is content, not structure. |
| Cursor placement | New described text replaces the edit and places the cursor at its end. |
| Dark theme | The frame paints dark panels, so its text no longer follows a light system theme. |
| Emphasized lines | An emphasized `:line` uses the strong text color. |

## AOT limits

The AOT shell model now compiles nine shell namespaces, including `bulwark.completion` and `bulwark.minibuffer`.
It runs the score, ranking, prompt transition, and prompt description assertions.
The task and driver functions are async, so the AOT binary interprets them. The bridge suite covers them.

The pinned compiler `540e4218` miscompiled the first form of `completion/rank`.
That form walked the candidates in a `loop` with `(seq ...)`, `first`, and `next` inside a `let`.
In the AOT binary, `first` returned nil for a nonempty sequence. Both evaluator modes passed.
The current form uses `reduce` and passes in the AOT binary.
Two smaller standalone loops did not reproduce the defect. No upstream issue exists yet.

## Checks and remaining evidence

| Check | Evidence |
|---|---|
| Portable workspace | 54 tests. The new suite runs in both evaluator modes. |
| `clj/tests/shell-model.cljrs` | Scores, ranking, every prompt transition, paging, and the prompt's elements and structure. Interpreter, Tiered, and AOT. |
| `clj/tests/minibuffer.cljrs` | Live `M-x` through `core/dispatch!`, docstrings, Unicode input, selection keys, completion, live redefinition, cancellation, focus restoration, nested and replaced prompts, literal URLs, history, obsolete answers, and failures. |
| Progress during a prompt | A page event, a prefix, and another command run while `M-x` is open. |
| Native unit tests | 14 shell tests and 3 Servo tests. The input test covers the label and cursor placement. |
| Native processes | Eleven cases per evaluator mode. The new `minibuffer` case uses the real frame and page focus. |
| Native nREPL, mock nREPL, both smokes, Servo smoke | Passed in both evaluator modes. |
| Formatting and Clippy | Portable workspace and native targets. |

All results come from local macOS runs on 2026-11-13. Linux CI has not run on this change.
[Run 43373187308](https://github.com/csm/bulwark/actions/runs/43373187308) passed on `main` at `dcf9a23`, so P2.4 now has Linux CI evidence.

The native case sends key and interaction events through `core/dispatch!`. It sends no physical key.
`python3 experiments/run_product_native.py --artifacts-dir DIR` retains `minibuffer-<mode>.png`.
That frame shows the label, the completed input with its cursor at the end, and the candidate lines.

Physical keyboard checks remain open:

```sh
cargo run --locked -- --nrepl --profile /tmp/bulwark-p25-physical https://example.com
```

1. Press `M-x`. The prompt must replace the echo line and take the keyboard. Use `C-c x` if `M-x` opens nothing.
2. Type `relo`. The first line must show `reload` and its docstring. The page must receive no key.
3. Press `ArrowDown`, `C-p`, and `Tab`. The selection must move, and the input must take the candidate with the cursor at its end.
4. Press `Enter`. The page must reload, and the next typed key must reach the page.
5. Press `C-l`, type a host name with a non-ASCII character through the input method, and press `Enter`.
6. Press `C-x b`, then `Escape`. The prompt must close, and the page must keep the keyboard.
7. Press `M-x`, then hold a page animation in view. The page must keep painting while the prompt is open.

On macOS, the Option key can change the logical key. `M-x` then arrives as another stroke and opens nothing.
winit can treat Option as Alt, but pages then lose Option characters. That choice needs an owner decision.
IME composition and clipboard use in the control also have no evidence yet.

After these checks, the next milestone is P2.6: introspection, messages, and configuration.
