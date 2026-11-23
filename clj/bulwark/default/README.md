# Default configuration

`bulwark.default.browsing` installs browser bindings, title history, pointer selection, and the last-buffer policy.
`bulwark.default.pages` installs find, zoom, link selection, and popup policy.
`bulwark.default.shell` installs echo hooks, error recovery, and Messages.
These namespaces use the same public APIs as trusted init files.
`bulwark.browser` retains the public browser helpers and compatible command names.

The shell loads `<profile>/init.cljrs` after defaults. The default profile is `~/.bulwark`.
`--profile` selects a profile without an nREPL requirement. The listener remains opt-in.
`M-x reload-init` reloads trusted configuration.
On macOS, Option preserves native character input by default. Other platforms retain Alt as Meta.
To use Option for `M-` bindings, add this form to `init.cljrs`:

```clojure
(bulwark.keymap/set-option-mode! :meta)
```

Use `:characters` for native character input or `:platform-default` to restore the platform default.
The setter also works in the internal REPL and applies without restarting.
Use the setter instead of changing the `bulwark.keymap/option-mode` atom directly.
In character mode, `C-c x` opens the command palette.
Escape cancels an open prompt immediately, including during a pending command prefix.
Outside prompts, Escape prefixes a Meta stroke: `Escape x` invokes `M-x` in either Option mode.
`Escape Escape` cancels that prefix. `C-g` cancels a pending prefix and otherwise cancels an open prompt.

The [P2.6 checkpoint](../../../docs/phase-2-introspection.md) documents Help, Messages, and the example reader configuration.
The [P2.7 checkpoint](../../../docs/phase-2-browsing.md) documents all default keys, popup ownership, and the internal REPL.
