# Default configuration

`bulwark.default.browsing` installs browser bindings, title history, pointer selection, and the last-buffer policy.
`bulwark.default.pages` installs find, zoom, link selection, and popup policy.
`bulwark.default.shell` installs echo hooks, error recovery, and Messages.
These namespaces use the same public APIs as trusted init files.
`bulwark.browser` retains the public browser helpers and compatible command names.

The shell loads `<profile>/init.cljrs` after defaults. The default profile is `~/.bulwark`.
`--profile` selects a profile without an nREPL requirement. The listener remains opt-in.
`M-x reload-init` reloads trusted configuration.
The [P2.6 checkpoint](../../../docs/phase-2-introspection.md) documents Help, Messages, and the example reader configuration.
The [P2.7 checkpoint](../../../docs/phase-2-browsing.md) documents all default keys, popup ownership, and the internal REPL.
