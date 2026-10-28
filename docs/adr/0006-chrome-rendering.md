# D6: egui for initial chrome

Status: accepted for Phases 1 and 2, 2026-10-27. Owner decision Q3.

The initial echo area, modeline, minibuffer, command palette, and REPL use egui.
Clojurust describes the UI as data and defines its behavior. Rust renders that data with egui.
Window layouts remain Clojurust data. Rust applies the resulting rectangles to native views and panels.

This choice follows the plan's Option A and the reference Servo shell.
It permits initial chrome development before a privileged webview bridge is available.

User-defined buffers rendered through Servo remain conditional Phase 3 work.
That work requires E5 results and a secure privileged bridge.
