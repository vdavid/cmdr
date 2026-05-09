/**
 * ESLint rule: ban raw `invoke('command_name', …)` calls outside the typed-IPC
 * bindings folder.
 *
 * Rationale: tauri command names are duplicated across the Rust `#[tauri::command]`
 * site and every TS call site, with no compile-time link. Renaming the Rust side
 * silently breaks runtime IPC. Generated bindings (`apps/desktop/src/lib/ipc/`)
 * give you typed `commands.commandName(args)` calls that fail at compile time.
 *
 * The bindings folder is allowed to call `invoke()` because that's what the
 * generated code does internally. Test files and dev-only debug panels are
 * also allowed (see allowedPathFragments).
 *
 * Mirrors the spirit of the Rust-side `error-string-match` check: keep magic
 * strings out of cross-layer dispatch.
 *
 * Opt out per-line with the standard ESLint comment if you really must:
 *
 *   // eslint-disable-next-line custom/no-raw-tauri-invoke -- <reason>
 */

// Path fragments that opt a file out entirely. Bindings need raw invoke;
// debug routes are dev-only; tests sometimes mock the IPC layer.
const allowedPathFragments = ['/lib/ipc/', '/routes/debug/', '.test.', '/test/']

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'problem',
    docs: {
      description: "Use the typed bindings in `lib/ipc/` instead of raw `invoke('name', ...)`.",
      recommended: true,
    },
    messages: {
      rawInvoke:
        "Don't call `invoke('{{ command }}')` directly. Import the typed binding from `$lib/ipc` " +
        '(or wherever the generated bindings module lives) so command names are checked at compile time. ' +
        'See AGENTS.md § "Type-safe IPC".',
    },
    schema: [],
  },
  create(context) {
    const filename = context.filename || context.getFilename() || ''
    if (allowedPathFragments.some((fragment) => filename.includes(fragment))) {
      return {}
    }

    return {
      CallExpression(node) {
        const callee = node.callee
        // Match plain `invoke(...)`. Renamed imports (`import { invoke as foo }`)
        // bypass this check, which is fine — that's a deliberate workaround that
        // already requires extra effort and reads as suspicious in review.
        if (callee.type !== 'Identifier' || callee.name !== 'invoke') return
        if (node.arguments.length < 1) return
        const firstArg = node.arguments[0]
        if (firstArg.type !== 'Literal' || typeof firstArg.value !== 'string') return

        context.report({
          node,
          messageId: 'rawInvoke',
          data: { command: firstArg.value },
        })
      },
    }
  },
}
