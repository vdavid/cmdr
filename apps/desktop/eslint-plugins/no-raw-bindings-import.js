/**
 * ESLint rule: ban importing the generated `commands` / `events` runtime bindings
 * from `$lib/ipc/bindings` outside the wrapper layer.
 *
 * Rationale: `$lib/tauri-commands/` is the canonical, typed seam for backend
 * communication. Each wrapper delegates to `commands.commandName(...)` /
 * `events.*`, unwrapping `Result<T, E>` via `throwIpcError`, handling the
 * `TimedOut<T>` vs `IpcError` split, and keeping macOS-only fallbacks in one
 * place. Feature code that reaches past it into the raw `commands`/`events`
 * bindings re-implements that unwrapping ad hoc and drifts from the seam, so the
 * discipline rots silently. This is the import-level twin of `no-raw-tauri-invoke`
 * (which bans raw `invoke('name', …)`): keep the whole app on the wrapper seam.
 *
 * ## What this rule catches
 *
 * A VALUE import of the `commands` or `events` specifier from the bindings module:
 *   - `import { commands } from '$lib/ipc/bindings'`
 *   - `import { commands, type RepoInfo } from '$lib/ipc/bindings'`
 *   - `import { commands as ipcCommands } from '$lib/ipc/bindings'`
 *   - `import { events } from '$lib/ipc/bindings'`
 *
 * ## What it deliberately does NOT catch
 *
 * - `import type { … }` (whole-declaration) and inline `type` specifiers.
 *   `bindings.ts` is the generated source of truth for IPC types, and the wrapper
 *   layer doesn't re-export all of them, so type imports from here are correct and
 *   expected everywhere.
 * - The bindings module and the wrapper layer themselves (`/lib/ipc/`,
 *   `/tauri-commands/`): that's where `commands`/`events` legitimately live and
 *   get wrapped.
 * - Test files (`.test.`, `/test/`) and dev-only debug routes (`/routes/debug/`):
 *   tests mock the bindings; debug panels are dev-only, same as `no-raw-tauri-invoke`.
 *
 * Opt out per-line, with a reason, only for genuine infra that must talk to the
 * raw bindings directly:
 *
 *   // eslint-disable-next-line cmdr/no-raw-bindings-import -- <why a wrapper doesn't fit>
 */

// Path fragments that opt a file out entirely. The bindings module and the
// wrapper layer are where `commands`/`events` legitimately live; tests mock them;
// debug routes are dev-only.
const allowedPathFragments = ['/lib/ipc/', '/tauri-commands/', '.test.', '/test/', '/routes/debug/']

// The runtime binding objects that must go through a wrapper. Generated types are
// fine to import from here; these two values are not.
const BANNED_VALUE_IMPORTS = new Set(['commands', 'events'])

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description: 'Import typed wrappers from `$lib/tauri-commands` instead of the raw `commands`/`events` bindings.',
      recommended: true,
    },
    messages: {
      rawBindingsImport:
        "Don't import `{{ name }}` from `$lib/ipc/bindings` directly. Use the typed wrapper from " +
        "`$lib/tauri-commands` (add a thin one if it's missing) so IPC stays on one seam with consistent " +
        'error/timeout unwrapping. See `lib/tauri-commands/CLAUDE.md`. Type imports from bindings are fine.',
    },
    schema: [],
  },
  create(context) {
    const filename = context.filename || context.getFilename() || ''
    if (allowedPathFragments.some((fragment) => filename.includes(fragment))) {
      return {}
    }

    return {
      ImportDeclaration(node) {
        // Only the bindings module. Matches the `$lib` alias and any relative
        // path resolving to `ipc/bindings`.
        const source = node.source.value
        if (typeof source !== 'string') return
        const isBindings = source === '$lib/ipc/bindings' || /(^|\/)ipc\/bindings$/.test(source)
        if (!isBindings) return

        // `import type { … }` is a whole-declaration type import — always fine.
        if (node.importKind === 'type') return

        for (const spec of node.specifiers) {
          if (spec.type !== 'ImportSpecifier') continue
          // Inline `import { type Foo }` specifiers are type-only — fine.
          if (spec.importKind === 'type') continue
          if (!BANNED_VALUE_IMPORTS.has(spec.imported.name)) continue

          context.report({
            node: spec,
            messageId: 'rawBindingsImport',
            data: { name: spec.imported.name },
          })
        }
      },
    }
  },
}
