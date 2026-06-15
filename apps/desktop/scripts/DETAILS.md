# Desktop scripts details

Depth and rationale. `CLAUDE.md` holds the must-knows; the wrapper architecture and decisions live here. The canonical
instance-isolation reference (per-resource derivation, race-window analysis, debug recipes, acceptance smoke) is
[`docs/tooling/instance-isolation.md`](../../../docs/tooling/instance-isolation.md).

## The wrapper architecture

`tauri-wrapper.js` is the single composition point for dev vs prod. Pure helpers in `instance-id.js` do the work so they
stay testable. For `pnpm dev`, the wrapper resolves an instance ID (from `--worktree <slug>`, the existing
`CMDR_INSTANCE_ID` env, or the default `"dev"`), reserves ephemeral ports, composes the bundle identifier + productName

- data dir + generated config payload, writes the config to a `$TMPDIR/cmdr-tauri-instance-<rand>/tauri.instance.json`
  (NOT in the repo, so a crashed wrapper can't pollute tracked space), writes the tauri-MCP port file BEFORE Tauri
  launches (the plugin has no bound-port accessor, so external readers learn the port from the wrapper), and exports
  `CMDR_DATA_DIR` + `CMDR_SECRET_STORE=file` for non-prod. Production leaves `CMDR_INSTANCE_ID` unset and runs
  byte-identical to before instance isolation existed.

## Key decisions

- **Pure helpers in `instance-id.js`, side effects in `tauri-wrapper.js`.** The sanitizer, identifier composer,
  port-file writer, and config-payload builder are all unit-testable in isolation. The wrapper is ~200 lines of obvious
  orchestration. Touching either side without breaking the other is the goal.
- **Generated `tauri.instance.json` lives in `$TMPDIR`, not the repo.** A crashed wrapper leaves the file behind;
  tracked space is sacred and `/tmp` self-prunes on macOS, so `.gitignore` needs no entry.
- **Ephemeral Vite + tauri-MCP ports are picked by the wrapper** via `net.createServer().listen(0)`, NOT by the
  consumers, because the wrapper knows the data dir and can write the port file BEFORE the consumer spawns. The race
  window (close → spawn → bind) is mitigated per-consumer: Vite uses `strictPort: true` so any collision is loud, and
  the Tauri-MCP plugin gets a post-bind connect-check on the Rust side that warns on mismatch.
