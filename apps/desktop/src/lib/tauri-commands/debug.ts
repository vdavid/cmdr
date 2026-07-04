// Dev/benchmark IPC: logs a frontend timing event into the unified Rust
// benchmark timeline (only active when `RUSTY_COMMANDER_BENCHMARK=1` is set).

import { commands } from '$lib/ipc/bindings'

/** Logs a frontend benchmark event to stderr, joining Rust's benchmark timeline. */
export function benchmarkLog(message: string): Promise<void> {
  return commands.benchmarkLog(message)
}
