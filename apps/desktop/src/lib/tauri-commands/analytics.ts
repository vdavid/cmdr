// Analytics commands

import { commands } from '$lib/ipc/bindings'

/**
 * Records a frontend-originated PostHog feature event through the single backend analytics path.
 * Fire-and-forget: the backend gates it (consent + dev/CI suppression + missing-key no-op), so call
 * it unconditionally.
 *
 * `props` must be a PII-free map of enums, counts, and bools only: never paths, file names, search
 * queries, prompts, or hostnames. It's serialized to JSON for the IPC boundary (the prop set is open
 * and can't be a fixed type). A debug-build backend guard warns if a prop value looks PII-shaped.
 */
export async function trackEvent(name: string, props: Record<string, string | number | boolean> = {}): Promise<void> {
  try {
    await commands.trackEvent(name, JSON.stringify(props))
  } catch {
    // Analytics is best-effort: never let a failed event surface to the user or break a flow.
  }
}
