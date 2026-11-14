# Default configuration

The native startup and echo hooks live in `bulwark.shell`.
The browser commands and title history live in `bulwark.browser`.
The default keys are `C-c r` for reload, `C-c b` for back, `C-c f` for forward, and `C-x h` for history.
The [command checkpoint](../../../docs/phase-1-commands.md) describes the command registry, keymap, hooks, and startup scripts.
The shell loads `<profile>/init.cljrs` after the defaults. The default profile is `~/.bulwark`.
`--profile` selects a profile without an nREPL requirement. The nREPL listener remains opt-in.
`M-x reload-init` reloads trusted configuration. `M-x show-messages` shows retained errors.
The [P2.6 checkpoint](../../../docs/phase-2-introspection.md) documents help commands and the example reader configuration.
The built-in `web-mode`, `text-mode`, and `repl-mode` live in `bulwark.browser` and bind no keys.
The [mode checkpoint](../../../docs/phase-2-modes.md) describes mode definitions and keymap precedence.
