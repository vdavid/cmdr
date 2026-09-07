/**
 * The one sign-in sheet, and the promise a caller awaits for its answer.
 *
 * ❗ **One sheet, mounted once** from `routes/(main)/+layout.svelte`. Every
 * credential ask in the app goes through `openSignInSheet`: the hub's Add row,
 * ⌘K, the pane's "Sign in…" banner, a saved row's Edit, and go-to-path handing
 * over a pasted address. A second dialog for the next protocol is what this
 * exists to prevent.
 *
 * ❗ **It resolves on the user's answer, not on a call returning.** The rest of
 * the app's dialog flows are a `$state` flag plus an `onClose` callback; this one
 * hands back a promise because `connect-flow.ts` awaits the sheet INSIDE a
 * connect, and a callback there would split one flow across two call stacks.
 *
 * ❗ **The sheet opens only on user intent** (`DETAILS.md` § "The four rules",
 * rule 4): activating a row that needs a sign-in, pressing Add,
 * pressing "Sign in…". ❌ Never on its own when a session drops. A modal stealing
 * focus during a lid-open wake is the wrong thing, and the pane's banner is where
 * that offer belongs.
 */

import type { SignInSheetRequest, SignInSheetResult } from './sign-in-contract'

const CANCELLED: SignInSheetResult = { kind: 'cancelled' }

interface OpenSheet {
  request: SignInSheetRequest
  /** Settles the caller's promise. Called exactly once, by `closeSignInSheet`. */
  settle: (result: SignInSheetResult) => void
}

const sheet = $state<{ open: OpenSheet | null }>({ open: null })

/** The request the mounted sheet should render, or `null` when nothing is open. */
export function currentSignInRequest(): SignInSheetRequest | null {
  return sheet.open?.request ?? null
}

/**
 * Opens the sheet and resolves once the user is done with it.
 *
 * ❗ A second open while one is up closes the first as `cancelled`, so its caller
 * never waits forever. That is a bug in the caller rather than a user action, and
 * it can happen for real: a switcher row activated twice while the first sheet is
 * still dialing.
 */
export function openSignInSheet(request: SignInSheetRequest): Promise<SignInSheetResult> {
  sheet.open?.settle(CANCELLED)
  return new Promise<SignInSheetResult>((resolve) => {
    sheet.open = { request, settle: resolve }
  })
}

/** Closes the sheet with `result`. Closing one that isn't open is a no-op. */
export function closeSignInSheet(result: SignInSheetResult): void {
  const open = sheet.open
  if (!open) return
  sheet.open = null
  open.settle(result)
}
