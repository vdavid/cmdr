/**
 * `virtual:cmdr-demo-void-commands`: every command the generated bindings (`src/lib/ipc/bindings.ts`)
 * declare as answering nothing, read at build time. The browser demo resolves an unanswered void
 * command and rejects any other, as Tauri does for a command it lacks
 * (`src/lib/demo/unanswered.ts`), so a `null` never poses as a real answer.
 *
 * Derived, never listed by hand: a command that changes shape changes here on the next build. Read
 * at build time because the bindings are about 1 MB, which no visitor should download.
 */
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

export const VOID_COMMANDS_MODULE_ID = 'virtual:cmdr-demo-void-commands'

/**
 * The void commands in a bindings source: a plain `__TAURI_INVOKE<void>('name'`, and a
 * `typedError<null, E>(__TAURI_INVOKE('name'` (a `Result<(), E>`, whose success carries `null`). The
 * second may wrap across lines, so the match spans whitespace up to its own invoke.
 */
export function voidCommandNames(bindingsSource: string): string[] {
  const names = new Set<string>()
  for (const match of bindingsSource.matchAll(/__TAURI_INVOKE<void>\(\s*'([a-z0-9_]+)'/g)) names.add(match[1])
  for (const match of bindingsSource.matchAll(
    /typedError<\s*null\s*,[^()]*?>\(\s*__TAURI_INVOKE\(\s*'([a-z0-9_]+)'/g,
  )) {
    names.add(match[1])
  }
  return [...names].sort()
}

interface LoadContext {
  addWatchFile: (file: string) => void
}

/** The plugin. `bindingsPath` is for tests; the app reads `src/lib/ipc/bindings.ts`. */
export function demoVoidCommands(options: { bindingsPath?: string } = {}): {
  name: string
  enforce: 'pre'
  resolveId: (id: string) => string | null
  load: (this: LoadContext, id: string) => string | null
} {
  const bindingsPath = options.bindingsPath ?? join(import.meta.dirname, '..', 'src', 'lib', 'ipc', 'bindings.ts')
  return {
    name: 'cmdr:demo-void-commands',
    enforce: 'pre',
    resolveId(id: string) {
      return id === VOID_COMMANDS_MODULE_ID ? `\0${id}` : null
    },
    load(id: string) {
      if (id !== `\0${VOID_COMMANDS_MODULE_ID}`) return null
      this.addWatchFile(bindingsPath)
      const names = voidCommandNames(readFileSync(bindingsPath, 'utf8'))
      return `export default new Set(${JSON.stringify(names)})\n`
    },
  }
}
