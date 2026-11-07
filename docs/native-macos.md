# Native macOS window test

Status: E3 single-process baseline passed, 2026-11-06.
Platform: macOS 26.5.2 arm64, Rust 1.97.1, Servo 0.7.0, debug build.

## Observed behavior

The native window displayed the local input fixture, servo.org, and English Wikipedia.
Computer Use operated the window through macOS input events.
The test used screenshots and native window titles to observe page behavior.

| Action | Observation | Result |
|---|---|---|
| Click the fixture button | The native window title changed to `Clicked`. | Passed |
| Type in the fixture input | The input and window title showed `native test`. | Passed |
| Edit the input | Backspace removed a character, and Shift+Arrow selected text for replacement. | Passed |
| Scroll the fixture | The bottom `End marker` became visible. | Passed |
| Resize the fixture window | The window changed from 800×632 to 650×482 points. The button moved to another line. | Passed |
| Scroll after resize | The heading and input appeared with the edited text intact. | Passed |
| Close the fixture | The process exited with status zero. | Passed |
| Click `Get Involved` on servo.org | The URL changed to `/contributing/`, and the page displayed its contribution instructions. | Passed |
| Click `Premises` on Wikipedia | The URL changed to `/wiki/Premise`, and the article title and body appeared. | Passed |

The two website checks establish basic rendering and link navigation.
They do not establish full site compatibility.
Wikipedia still displays black squares in place of some icons, as the earlier screenshot runs showed.
The probes also emit the existing GL texture warning.

The accessibility tree contains the native window and its controls, but no page content.
This probe therefore needs coordinate actions for page controls.
E3 does not yet connect Servo's accessibility API to the native host.
IME, clipboard integration, alternate keyboard layouts, and display-scale transitions remain untested.

## Reproduction

Build the native probe:

```sh
cargo build --locked -p bulwark-experiment-e3 --features servo
```

Run the local fixture:

```sh
target/debug/bulwark-experiment-e3 "file://$PWD/experiments/fixtures/input.html"
```

Run each website with the same binary:

```sh
target/debug/bulwark-experiment-e3 https://servo.org
target/debug/bulwark-experiment-e3 https://en.wikipedia.org
```

On macOS, these commands need access to the native window server outside the Codex filesystem sandbox.
The sandboxed attempt could not connect to macOS services and did not display a test window.
The successful automation used a temporary `.app` bundle with the compiled binary and an identifying `Info.plist`.
The bundle changed process identification for Computer Use. It did not change the probe code.
The build took 2m 31s with cached dependencies and an empty target directory.

## Development direction

E3 now satisfies the initial macOS rendering and input milestone.
The [native E7 continuation](native-async-bridge.md) now implements native wakeups and explicit failure and shutdown behavior.
The upstream async adoption checks now pass. The [Phase 0 exit review](phase-0-exit.md) records product entry and remaining limits.

The initial app can use one visible webview per window.
E5 remains necessary before web content splits, but does not block this initial layout.
Linux and multiprocess validation remain follow-up work.
