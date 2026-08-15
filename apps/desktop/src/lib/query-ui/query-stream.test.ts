/**
 * The pure half of a streaming run: the status sentences and the announcement
 * throttle.
 *
 * The throttle is the one an audit can't catch. A live run emits a batch every
 * 100 ms into an `aria-live` region; axe sees a well-formed live region and passes,
 * while a screen-reader user hears a number read out ten times a second. So the rule
 * gets its own test rather than riding along with the a11y suite.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import {
  createAnnouncementThrottle,
  liveStatusLine,
  liveWalkProgress,
  livePhaseLabel,
  LIVE_ANNOUNCE_INTERVAL_MS,
  type LiveRunView,
} from './query-stream'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

function view(overrides: Partial<LiveRunView> = {}): LiveRunView {
  return {
    phase: 'walking',
    matchCount: 12,
    dirsFound: 340,
    currentPath: '/Volumes/naspi/photos',
    capped: false,
    running: true,
    incomplete: false,
    ...overrides,
  }
}

describe('createAnnouncementThrottle', () => {
  it('announces the first update, then stays quiet for two seconds', () => {
    let clock = 1_000
    const throttle = createAnnouncementThrottle(() => clock)

    expect(throttle.offer('12 matches so far', false)).toBe(true)
    expect(throttle.text).toBe('12 matches so far')

    clock += 100
    expect(throttle.offer('98 matches so far', false)).toBe(false)
    clock += 100
    expect(throttle.offer('210 matches so far', false)).toBe(false)
    // What it says is still the announcement from the start of the window.
    expect(throttle.text).toBe('12 matches so far')
  })

  it('lets the next update through once the interval has passed', () => {
    let clock = 1_000
    const throttle = createAnnouncementThrottle(() => clock)
    throttle.offer('12 matches so far', false)

    clock += LIVE_ANNOUNCE_INTERVAL_MS - 1
    expect(throttle.offer('900 matches so far', false)).toBe(false)
    clock += 1
    expect(throttle.offer('901 matches so far', false)).toBe(true)
    expect(throttle.text).toBe('901 matches so far')
  })

  it('always announces the final word, however soon it lands', () => {
    // The interesting case is the fast run: one batch, then done, inside the same
    // window. Without the `final` bypass the user hears the running count and never
    // hears that the search finished.
    let clock = 1_000
    const throttle = createAnnouncementThrottle(() => clock)
    throttle.offer('12 matches so far', false)

    clock += 30
    expect(throttle.offer('12 of 12 results', true)).toBe(true)
    expect(throttle.text).toBe('12 of 12 results')
  })

  it('says nothing twice: an unchanged sentence is not an announcement', () => {
    let clock = 1_000
    const throttle = createAnnouncementThrottle(() => clock)
    throttle.offer('12 matches so far', false)
    clock += 10_000
    expect(throttle.offer('12 matches so far', true)).toBe(false)
  })
})

describe('liveStatusLine', () => {
  it('counts up without ever claiming a total while the run is going', () => {
    // "N so far" is the whole point: a live count can still rise, and a count-only
    // run over a live walk can even over-count its overlap. Neither may read as final.
    expect(liveStatusLine(view({ matchCount: 1234 }), 100)).toBe('1,234 matches so far')
    expect(liveStatusLine(view({ matchCount: 1 }), 1)).toBe('1 match so far')
  })

  it('admits it did not finish when the run ended short', () => {
    const line = liveStatusLine(view({ running: false, incomplete: true, matchCount: 40 }), 12)
    expect(line).toBe("12 of 40 results. Cmdr didn't finish looking.")
  })

  it('drops the arithmetic when a stopped run had found nothing', () => {
    // Found driving the app: stopping a slow search before anything matched read as
    // "0 of 0 results. Cmdr didn't finish looking.", which is two numbers saying nothing.
    const line = liveStatusLine(view({ running: false, incomplete: true, matchCount: 0 }), 0)
    expect(line).toBe("Nothing found before this search stopped. Cmdr didn't finish looking.")
  })

  it('says the rows stopped at the cap while the count carried on', () => {
    const line = liveStatusLine(view({ running: false, capped: true, matchCount: 5000 }), 1000)
    expect(line).toBe('Showing the first 1,000 of 5,000 matches.')
  })

  it('leaves a covered run to the ordinary result line', () => {
    expect(liveStatusLine(view({ running: false }), 12)).toBe('')
  })

  it('does not claim truncation when the run stopped exactly ON the cap', () => {
    // `capped` is "the row cap was reached", which is true the moment the last row
    // fits, so a run whose matches happen to total exactly the cap reports it. There
    // is nothing behind that row, and "Showing the first 30 of 30 matches" reads as
    // if there were. Found end to end: the live-walk E2E searches a 30-file tree
    // through the dialog's 30-row cap.
    expect(liveStatusLine(view({ running: false, capped: true, matchCount: 30 }), 30)).toBe('')
  })
})

describe('liveWalkProgress and livePhaseLabel', () => {
  it('reports directories scanned only while the walk is the thing running', () => {
    expect(liveWalkProgress(view({ dirsFound: 4312 }))).toBe('4,312 folders scanned')
    expect(liveWalkProgress(view({ phase: 'readingIndex' }))).toBe('')
    expect(liveWalkProgress(view({ running: false }))).toBe('')
  })

  it('gives each phase its own honest sentence', () => {
    const labels = (['resolvingCoverage', 'waitingForAnotherWalk', 'readingIndex', 'walking'] as const).map(
      livePhaseLabel,
    )
    expect(new Set(labels).size).toBe(4)
    expect(labels.every((l) => l.length > 0)).toBe(true)
  })

  it('never counts folders for a run that is queued behind another walk', () => {
    // The run holds no ground, so it is scanning nothing. "0 folders scanned" beside a
    // sentence about waiting reads as a stuck walk.
    expect(liveWalkProgress(view({ phase: 'waitingForAnotherWalk', dirsFound: 0 }))).toBe('')
  })
})
