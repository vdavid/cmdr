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

/**
 * A count as a coarse bucket, mirroring the backend's `analytics::item_count_bucket`
 * (`apps/desktop/src-tauri/src/analytics/mod.rs`) EXACTLY.
 *
 * Two copies exist because a frontend event never crosses the backend helper on its
 * way out, and one product with two ideas of what "a lot" means makes the dashboard
 * unreadable. `item_count_bucket_matches_the_backend` pins the boundaries; change
 * one side and that test tells you about the other.
 *
 * ❌ Don't invent a per-feature bucketing next to a call to this. The one documented
 * exception is a count with a hard low cap of its own (open tabs cap at ten, where
 * this ladder has two values across the whole range) — those say so at the call site.
 */
export function itemCountBucket(count: number): string {
  if (count <= 0) return '0'
  if (count === 1) return '1'
  if (count <= 10) return '2-10'
  if (count <= 100) return '11-100'
  if (count <= 1000) return '101-1000'
  return '1000+'
}
