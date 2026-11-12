/**
 * When to stop showing a confident ETA, and what to say instead.
 *
 * The regression: the copy dialog claimed "~8m 12s remaining" throughout a
 * total stall on 2026-07-31. The backend now classifies what a transfer is
 * waiting on (`TransferActivity`, from the in-flight probe); this module owns
 * the presentation decisions on top of it: how long to wait before calling a
 * transfer stalled, and what to say while bytes are genuinely on their way. Deliberately NOT a second stall detector: it never looks at event
 * timing, only at what the backend reported.
 */
import { describe, it, expect } from 'vitest'
import type { TransferActivity } from '#lib/tauri-commands/index.js'
import { STALL_NOTICE_SECONDS, stallNoticeFor, waitLineFor } from './transfer-stall'

function activity(over: Partial<TransferActivity> = {}): TransferActivity {
  return {
    inFlight: 0,
    stillForSeconds: 0,
    waitingOn: 'moving',
    openingSource: false,
    sourceInboundBytesPerSecond: null,
    reconnectingVolume: null,
    ...over,
  }
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

describe('stallNoticeFor, while the source is still sending', () => {
  it('holds the stall notice back while bytes are arriving', () => {
    // A response on its way is slow, not stopped: "the transfer has stopped
    // moving" over a live receive rate would be a contradiction on screen.
    const receiving = activity({ stillForSeconds: 45, waitingOn: 'source', sourceInboundBytesPerSecond: 19_000 })
    expect(stallNoticeFor(receiving)).toBeNull()
  })

  it('speaks again once nothing is arriving either', () => {
    const silent = activity({ stillForSeconds: 45, waitingOn: 'source', openingSource: true })
    expect(stallNoticeFor(silent)?.reason).toBe('source')
  })

  it("doesn't claim a partial write while nothing has landed yet", () => {
    // ERR-CNK7M: the only task was still opening its source, so "1 file is
    // still open and may already be partly written" was false.
    const opening = activity({ stillForSeconds: 12, waitingOn: 'source', openingSource: true, inFlight: 1 })
    expect(stallNoticeFor(opening)?.inFlight).toBe(0)
  })

  it('still counts open files once bytes have landed', () => {
    const writing = activity({ stillForSeconds: 12, waitingOn: 'destination', inFlight: 3 })
    expect(stallNoticeFor(writing)?.inFlight).toBe(3)
  })
})

describe('waitLineFor', () => {
  it('says the first file is being opened straight away, before the stall threshold', () => {
    // ERR-CNK7M: 20 s on a silent 0% bar. Before the first byte there's no ETA
    // to protect, so there's nothing to wait out.
    expect(waitLineFor(activity({ stillForSeconds: 1, waitingOn: 'moving', openingSource: true }))).toEqual({
      kind: 'opening',
      forSeconds: 1,
    })
    expect(
      waitLineFor(activity({ stillForSeconds: STALL_NOTICE_SECONDS - 1, waitingOn: 'source', openingSource: true })),
    ).toEqual({ kind: 'opening', forSeconds: STALL_NOTICE_SECONDS - 1 })
  })

  it('hands over to the stall notice once an open has taken long enough with nothing arriving', () => {
    expect(
      waitLineFor(activity({ stillForSeconds: STALL_NOTICE_SECONDS, waitingOn: 'source', openingSource: true })),
    ).toBeNull()
  })

  it('shows the receive rate whenever bytes are arriving that the bar cannot count yet', () => {
    expect(
      waitLineFor(
        activity({
          stillForSeconds: 30,
          waitingOn: 'source',
          openingSource: true,
          sourceInboundBytesPerSecond: 19_000,
        }),
      ),
    ).toEqual({ kind: 'receiving', bytesPerSecond: 19_000 })
  })

  it('says nothing about a deliberate wait', () => {
    expect(
      waitLineFor(activity({ waitingOn: 'paused', openingSource: true, sourceInboundBytesPerSecond: 5 })),
    ).toBeNull()
    expect(waitLineFor(activity({ waitingOn: 'conflict', openingSource: true }))).toBeNull()
  })

  it('says nothing for a transfer that is simply moving, or reports no activity', () => {
    expect(waitLineFor(activity())).toBeNull()
    expect(waitLineFor(null)).toBeNull()
  })

  // #398: a NAS that dropped off the network. The backend waits for it and says
  // which one, from the first second, on both surfaces.
  it('names the volume a transfer is waiting on to come back', () => {
    expect(waitLineFor(activity({ waitingOn: 'reconnecting', reconnectingVolume: 'Testnas' }))).toEqual({
      kind: 'reconnecting',
      volumeName: 'Testnas',
    })
  })

  it('puts the volume coming back ahead of an opening or a trickle under it', () => {
    // Nothing arrives from a volume that's gone, whatever a stale reading says.
    expect(
      waitLineFor(
        activity({
          waitingOn: 'reconnecting',
          reconnectingVolume: 'Testnas',
          openingSource: true,
          sourceInboundBytesPerSecond: 5,
        }),
      ),
    ).toEqual({ kind: 'reconnecting', volumeName: 'Testnas' })
  })
})

describe('stallNoticeFor while a volume comes back', () => {
  it('never calls waiting for a volume a stall', () => {
    // It says what it's waiting on, by name; "no progress for 2m" on top would
    // be true and useless.
    expect(
      stallNoticeFor(activity({ stillForSeconds: 120, waitingOn: 'reconnecting', reconnectingVolume: 'Testnas' })),
    ).toBeNull()
  })
})
