import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
import { voidCommandNames } from './vite-demo-void-commands.ts'

const bindings = readFileSync(join(import.meta.dirname, '..', 'src', 'lib', 'ipc', 'bindings.ts'), 'utf8')

describe('voidCommandNames', () => {
  it('finds the void and the `Result<(), E>` commands of the real bindings, and nothing that answers', () => {
    const names = voidCommandNames(bindings)
    expect(names).toContain('cancel_listing')
    expect(names).toContain('show_notification')
    for (const answering of ['get_index_status', 'get_dir_stats', 'get_dir_stats_batch', 'get_file_range']) {
      expect(names).not.toContain(answering)
    }
  })

  it('reads a `typedError<null, E>` wrapped across lines, and skips a non-null one beside it', () => {
    const source = [
      '  a: () => typedError<',
      '      null,',
      '      string',
      "    >(__TAURI_INVOKE('wrapped_void')),",
      "  b: () => typedError<{ x: number } | null, string>(__TAURI_INVOKE('answers_something')),",
      "  c: () => __TAURI_INVOKE<number>('answers_a_number'),",
    ].join('\n')
    expect(voidCommandNames(source)).toEqual(['wrapped_void'])
  })
})
