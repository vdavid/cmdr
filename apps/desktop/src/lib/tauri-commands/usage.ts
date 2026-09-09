// Launch-day ledger (read-only; Rust appends today's day at startup)

import { commands } from '$lib/ipc/bindings'

/**
 * How many distinct local calendar days Cmdr has been launched on, per the on-device ledger.
 *
 * The gate for usage-gated hints ("you've used Cmdr for a few days now"). The ledger never leaves
 * the Mac: it's not telemetry, and it isn't a setting either, so it can't be picked up by the
 * settings→PostHog auto-ship. See `src-tauri/src/usage/CLAUDE.md`.
 *
 * A missing, unreadable, or slow ledger answers 0, which keeps a hint silent rather than firing it
 * on a guess.
 */
export async function getLaunchDayCount(): Promise<number> {
  try {
    return await commands.getLaunchDayCount()
  } catch {
    // Reading the ledger is best-effort: a hint that stays quiet beats an error in someone's face.
    return 0
  }
}
