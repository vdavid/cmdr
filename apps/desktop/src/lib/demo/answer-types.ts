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

/**
 * One command's fake answer. To answer a `typedError` command's error branch, throw the typed
 * error value (a plain object or string, never an `Error`): `typedError` turns it into
 * `{ status: 'error', error }`, as it does for a refusal from Rust.
 */
export type Answer<K extends CommandName> = (payload: Payload) => RawAnswer<K> | Promise<RawAnswer<K>>

export type AnswerTable = { [K in CommandName]?: Answer<K> }

/** A command whose answer carries nothing (Rust returns `()`), so the demo can acknowledge it silently. */
export type VoidCommand = { [K in CommandName]: ReturnType<Commands[K]> extends Promise<void> ? K : never }[CommandName]
