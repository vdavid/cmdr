/**
 * The shared rule every unsolicited offer obeys: what Cmdr has already asked
 * for, and how long it must then hold its tongue.
 *
 * Pure, because this is the part worth pinning down: every nudge is once-ever,
 * so a wrong yes is unrecoverable and a wrong no is a feature nobody is offered.
 * Each nudge adds its own paid-for conditions on top ({@link nudgeCouldFire} is
 * the free prefix they share); the ledger's settings I/O is `nudge-store.ts`.
 */

/**
 * When an offer was made, as an ISO 8601 instant, or `''` when it never was.
 *
 * A date rather than a bool so one nudge can see how recently another one
 * spoke. An instant rather than a local day: the comparison is an elapsed
 * duration, which no calendar or timezone has an opinion about.
 */
export type NudgeOfferedAt = string

/** Every offer that participates in the shared floor. */
export type NudgeKind = 'dockPin' | 'reveal'

/** When each offer was last made. One entry per {@link NudgeKind}, always present. */
export type NudgeLedger = Record<NudgeKind, NudgeOfferedAt>

/**
 * How long one offer keeps every other offer quiet.
 *
 * Three days, so two nudges land as two separate moments rather than as a run
 * of prompts. This is a property of ALL nudges, ❌ never a rule about a
 * particular pair: adding a third offer must not mean adding two more rules.
 */
export const NUDGE_COOLDOWN_DAYS = 3

const DAY_MS = 24 * 60 * 60 * 1000

/** A ledger where nothing has been offered yet. */
export function emptyNudgeLedger(): NudgeLedger {
  return { dockPin: '', reveal: '' }
}

/**
 * Whether this offer has already been made.
 *
 * ❗ Any non-empty stamp counts, including one nothing can parse: a stored
 * value means the offer happened and only WHEN was lost, and reading it as
 * "never asked" would offer a once-ever thing twice.
 */
export function wasOffered(at: NudgeOfferedAt): boolean {
  return at !== ''
}

/**
 * Whether some offer was made recently enough to keep the next one waiting.
 *
 * Only a stamp that reads as a real, past instant can block. An unparseable one
 * and a future one (the clock moved backwards between the write and now) are
 * both ignored, because the alternative is a machine where no nudge ever fires
 * again and nothing says why.
 */
export function nudgeCooldownActive(ledger: NudgeLedger, now: Date): boolean {
  return Object.values(ledger).some((at) => {
    const elapsed = now.getTime() - Date.parse(at)
    return Number.isFinite(elapsed) && elapsed >= 0 && elapsed < NUDGE_COOLDOWN_DAYS * DAY_MS
  })
}

/** The half of any nudge's decision that costs nothing to ask. */
export interface NudgeContext {
  /** `isE2eRun()`: a Playwright shard or the screenshot capture pass. */
  automatedRun: boolean
  /** Whether this is a Mac at all. Every nudge here is about a macOS-only affordance. */
  onMacOs: boolean
  /** `onboarding.completed`. */
  onboarded: boolean
  /** Whether the onboarding wizard is on screen right now. */
  onboardingShowing: boolean
  /** What has been offered so far, from `nudge-store.ts::readNudgeLedger`. */
  ledger: NudgeLedger
  /** Injected so the cooldown is testable without waiting three days. */
  now: Date
}

/**
 * Whether `kind` is still worth asking the backend anything about.
 *
 * Split out of each nudge's full rule so the common case — every launch after
 * an offer has been answered — never pays for an IPC round trip. Each nudge's
 * `shouldShow…` calls it too, so the two can't drift.
 */
export function nudgeCouldFire(ctx: NudgeContext, kind: NudgeKind): boolean {
  if (ctx.automatedRun) return false
  if (!ctx.onMacOs) return false
  if (!ctx.onboarded) return false
  if (ctx.onboardingShowing) return false
  if (wasOffered(ctx.ledger[kind])) return false
  return !nudgeCooldownActive(ctx.ledger, ctx.now)
}
