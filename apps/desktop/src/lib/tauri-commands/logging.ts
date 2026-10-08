import { commands } from '$lib/ipc/bindings'

/** Uses the running app’s resolved log directory, including dev/worktree overrides. */
export function getDebugLogPath(): Promise<string | null> {
  return commands.getDebugLogPath()
}
