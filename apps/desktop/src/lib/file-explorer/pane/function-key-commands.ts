/**
 * The F-key bar's button → command-id map.
 *
 * Held as a typed constant (not inlined at each `onCommand?.(…)` call site) so
 * `cmdr/no-raw-command-dispatch` stays satisfied: every dispatch passes a typed
 * `CommandId`, never a magic string. Lives in its own module so the mapping is
 * unit-testable without mounting the Svelte component (`function-key-commands.test.ts`).
 */
import type { CommandId } from '$lib/commands'

export const fnKeyToCommand = {
  view: 'file.view',
  edit: 'file.edit',
  copy: 'file.copy',
  move: 'file.move',
  rename: 'file.rename',
  newFile: 'file.newFile',
  newFolder: 'file.newFolder',
  delete: 'file.delete',
  deletePermanently: 'file.deletePermanently',
} as const satisfies Record<string, CommandId>
