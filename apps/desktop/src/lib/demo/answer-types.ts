/**
 * Types that tie the demo's fake answers to the generated bindings, so `tsc` flags a drifted
 * answer the day a Rust command's return type changes.
 */
import type { commands } from '#lib/ipc/bindings.js'

type Commands = typeof commands

/** A command's name as the bindings spell it (`listVolumes`). */
export type CommandName = keyof Commands

/** What `invoke` resolves to for one command: the `data` of a `typedError` result, or the bare value. */
export type RawAnswer<K extends CommandName> =
  Awaited<ReturnType<Commands[K]>> extends infer R
    ? R extends { status: 'ok'; data: infer T }
      ? T
      : R extends { status: 'error'; error: unknown }
        ? never
        : R
    : never

/** The IPC payload: the bindings' positional args, keyed by their camelCase names. */
export type Payload = Record<string, unknown>

/** One command's fake answer. A `typedError` command's error branch is `refuse(typedError)`. */
export type Answer<K extends CommandName> = (payload: Payload) => RawAnswer<K> | Promise<RawAnswer<K>>

export type AnswerTable = { [K in CommandName]?: Answer<K> }

/** A `typedError` command's error type, as the bindings declare it. */
type RefusalOf<K extends CommandName> =
  Awaited<ReturnType<Commands[K]>> extends infer R ? (R extends { status: 'error'; error: infer E } ? E : never) : never

/**
 * Answers a `typedError` command with its error branch, typed against that command's bindings: `invoke` rejects with exactly this value,
 * as it does for a Rust `Err`, and the bindings hand it back as `{ status: 'error', error }`.
 */
export function refuse<K extends CommandName>(_command: K, error: RefusalOf<K>): never {
  // eslint-disable-next-line @typescript-eslint/only-throw-error -- the IPC wire carries the typed value, never an `Error`
  throw error
}

/** A command whose answer carries nothing (Rust returns `()`), so the demo can acknowledge it silently. */
export type VoidCommand = { [K in CommandName]: ReturnType<Commands[K]> extends Promise<void> ? K : never }[CommandName]
