/**
 * What one "Tell me about this" card reports when it closes (`tell_me_about_used`), as pure vocabulary.
 *
 * PII-free by construction: this file never sees a path, a name, a URL, an app, or the AI take's text. What crosses is
 * how the card was opened, what kind of item it was about and on which kind of volume, how its AI take ended (with the
 * typed reason when it had none), and what the person did next (`apps/desktop/src-tauri/src/analytics/CLAUDE.md`).
 *
 * One event per card, at its close, because only then is every answer in: whether the take finished, was stopped, or
 * was cut short by the close, and which action (if any) the person took.
 */

import type { AboutAction, AboutTarget } from '#lib/ipc/bindings.js'
import { trackEvent } from '#lib/tauri-commands/index.js'
import type { AboutCardData } from './about-card'
import { verdictPhase, type AboutVerdict } from './about-verdict'

/**
 * The road the card's command came in by: the dispatch core's `DispatchSource` token, passed by the opener. On macOS
 * the shortcut usually arrives as `menu` (the menu bar's accelerator fires before the webview's keydown), and the
 * context menu and the menu bar share one item id, so `menu` covers all three there; `palette` and `keyboard` are
 * exact.
 */
export type AboutTrigger = 'keyboard' | 'menu' | 'mouse' | 'palette' | 'explorer' | 'mcp'

/** What the opener knows about the card that the card itself doesn't. */
export interface AboutUsageContext {
  trigger: AboutTrigger
  /** The pane's `VolumeKind` for the item's volume, or `unknown` when the card placed a bare path. */
  volumeKind: string
}

/** What the person did to leave the card: a footer action, Enter on a row naming a place, or just a close. */
export type AboutExit = { type: 'action'; action: AboutAction['type'] } | { type: 'goTo' } | { type: 'closed' }

const TARGET_TOKENS: Record<AboutTarget, string> = {
  file: 'file',
  folder: 'folder',
  appBundle: 'app',
  package: 'package',
  symlink: 'symlink',
  archiveEntry: 'archive_entry',
}

/**
 * How the card's AI take ended, plus the typed reason it had none or couldn't finish (`none` otherwise), and the
 * model's trust tier when a take was planned (`none` otherwise).
 */
function verdictProps(card: AboutCardData): { verdict: string; verdict_why: string; verdict_trust: string } {
  const verdict = card.verdict
  if (verdict === null) return { verdict: 'none', verdict_why: 'none', verdict_trust: 'none' }
  if (verdict.status === 'unavailable') {
    return { verdict: 'unavailable', verdict_why: verdict.why.kind, verdict_trust: 'none' }
  }
  return { ...plannedProps(verdict), verdict_trust: verdict.trust }
}

function plannedProps(verdict: Extract<AboutVerdict, { status: 'planned' }>): { verdict: string; verdict_why: string } {
  const phase = verdictPhase(verdict)
  switch (phase) {
    case 'done':
      return { verdict: 'shown', verdict_why: 'none' }
    case 'fromEarlier':
      return { verdict: 'cached', verdict_why: 'none' }
    case 'stopped':
      return { verdict: 'stopped', verdict_why: 'none' }
    case 'failed':
      return { verdict: 'failed', verdict_why: verdict.ending?.kind === 'failed' ? verdict.ending.problem : 'other' }
    case 'waiting':
    case 'streaming':
      // Closed before the take ended: the close stopped it.
      return { verdict: 'closed_early', verdict_why: 'none' }
  }
}

/** The event's props for `card`, closed by `exit`. Pure, so the vocabulary is tested without IPC. */
export function aboutUsageProps(
  card: AboutCardData,
  context: AboutUsageContext,
  exit: AboutExit,
): Record<string, string | boolean> {
  const action = exit.type === 'action' ? exit.action : exit.type === 'goTo' ? 'goTo' : 'none'
  return {
    trigger: context.trigger,
    target: card.plan === null ? 'unknown' : TARGET_TOKENS[card.plan.target],
    volume_kind: context.volumeKind,
    unavailable: card.unavailable ?? 'none',
    settled: card.settled,
    ...verdictProps(card),
    action,
    continued: action === 'continueInAskCmdr',
  }
}

/** Reports one closed card. Best-effort, like every event. */
export function trackAboutUsed(card: AboutCardData, context: AboutUsageContext, exit: AboutExit): void {
  void trackEvent('tell_me_about_used', aboutUsageProps(card, context, exit))
}
