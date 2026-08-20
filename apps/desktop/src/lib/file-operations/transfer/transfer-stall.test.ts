/**
 * When to stop showing a confident ETA, and what to say instead.
 *
 * The regression: the copy dialog claimed "~8m 12s remaining" throughout a
 * total stall on 2026-07-31. The backend now classifies what a transfer is
 * waiting on (`TransferActivity`, from the in-flight probe); this module owns
 * the one presentation decision on top of it — how long to wait before
 * speaking. Deliberately NOT a second stall detector: it never looks at event
 * timing, only at what the backend reported.
 */
import { describe, it, expect } from 'vitest'
import type { TransferActivity } from '$lib/tauri-commands'
import { STALL_NOTICE_SECONDS, stallNoticeFor } from './transfer-stall'

function activity(over: Partial<TransferActivity> = {}): TransferActivity {
  return { inFlight: 0, stillForSeconds: 0, waitingOn: 'moving', ...over }
}

describe('stallNoticeFor', () => {
  it('says nothing while bytes are moving', () => {
    expect(stallNoticeFor(activity())).toBeNull()
  })

  it('says nothing for an operation that reports no activity at all', () => {
    // Local copy, delete, and trash keep no in-flight table. Silence, not a
    // guess.
    expect(stallNoticeFor(null)).toBeNull()
    expect(stallNoticeFor(undefined)).toBeNull()
  })

  it('stays quiet through a brief pause between chunks', () => {
    // A slow-but-alive transfer must not be accused of stalling; that's how a
    // warning gets trained into background noise.
    const notice = stallNoticeFor(activity({ stillForSeconds: STALL_NOTICE_SECONDS - 1, waitingOn: 'unknown' }))
    expect(notice).toBeNull()
  })

  it('speaks once the transfer has been still long enough', () => {
    const notice = stallNoticeFor(
      activity({ stillForSeconds: STALL_NOTICE_SECONDS, waitingOn: 'unknown', inFlight: 5 }),
    )
    expect(notice).not.toBeNull()
    expect(notice?.stillForSeconds).toBe(STALL_NOTICE_SECONDS)
    expect(notice?.inFlight).toBe(5)
  })

  it('never calls a deliberate pause a stall', () => {
    // The dialog already says "Paused" in its title. Adding "no progress for
    // 5m" would be technically true and completely wrong.
    expect(stallNoticeFor(activity({ stillForSeconds: 300, waitingOn: 'paused' }))).toBeNull()
  })

  it('never calls waiting for a person a stall', () => {
    // A conflict prompt is open: the transfer is doing exactly what it should.
    expect(stallNoticeFor(activity({ stillForSeconds: 300, waitingOn: 'conflict' }))).toBeNull()
  })

  it('names the side that stopped responding, so the message is actionable', () => {
    expect(stallNoticeFor(activity({ stillForSeconds: 60, waitingOn: 'destination' }))?.reason).toBe('destination')
    expect(stallNoticeFor(activity({ stillForSeconds: 60, waitingOn: 'source' }))?.reason).toBe('source')
    expect(stallNoticeFor(activity({ stillForSeconds: 60, waitingOn: 'unknown' }))?.reason).toBe('unknown')
  })

  it('reports in-flight files only when it has some to report', () => {
    // "0 files are still open" is noise; the line is there to explain a
    // counter that reads lower than what's visible at the destination.
    expect(stallNoticeFor(activity({ stillForSeconds: 60, waitingOn: 'unknown', inFlight: 0 }))?.inFlight).toBe(0)
    expect(stallNoticeFor(activity({ stillForSeconds: 60, waitingOn: 'unknown', inFlight: 3 }))?.inFlight).toBe(3)
  })
})
