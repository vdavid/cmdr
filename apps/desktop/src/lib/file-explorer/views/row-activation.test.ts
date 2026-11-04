import { describe, expect, it } from 'vitest'
import { createRowActivation, DOUBLE_TAP_MS } from './row-activation'

function setup() {
  const opened: number[] = []
  let time = 1_000
  let listingId = 'listing-1'
  const activation = createRowActivation({
    getListingId: () => listingId,
    activate: (index) => opened.push(index),
    now: () => time,
  })
  const press = (pointerType: string) => {
    activation.onPointerDown({ pointerType } as PointerEvent)
  }
  return {
    activation,
    opened,
    press,
    advance: (ms: number) => (time += ms),
    setListing: (id: string) => (listingId = id),
    /** One tap as Chrome's device mode delivers it: a press, then a plain click. */
    tap(index: number, pointerType = 'touch') {
      press(pointerType)
      activation.onClick(index)
    },
  }
}

describe('createRowActivation', () => {
  describe('mouse', () => {
    it('opens on dblclick, once', () => {
      const s = setup()
      s.tap(3, 'mouse')
      s.advance(100)
      s.tap(3, 'mouse')
      s.activation.onDoubleClick(3)
      expect(s.opened).toEqual([3])
    })

    it('never opens on two quick clicks alone: the OS decides what a double-click is', () => {
      const s = setup()
      s.tap(3, 'mouse')
      s.advance(100)
      s.tap(3, 'mouse')
      expect(s.opened).toEqual([])
    })

    it('opens on a dblclick with no press seen', () => {
      const s = setup()
      s.activation.onDoubleClick(2)
      expect(s.opened).toEqual([2])
    })
  })

  describe('touch and pen', () => {
    it('opens on two quick taps on the same row, with no dblclick', () => {
      const s = setup()
      s.tap(4)
      s.advance(150)
      s.tap(4)
      expect(s.opened).toEqual([4])
    })

    it('opens once when the browser also sends a dblclick', () => {
      const s = setup()
      s.tap(4)
      s.advance(150)
      s.tap(4)
      s.activation.onDoubleClick(4)
      expect(s.opened).toEqual([4])
    })

    it('works for a pen too', () => {
      const s = setup()
      s.tap(1, 'pen')
      s.advance(100)
      s.tap(1, 'pen')
      expect(s.opened).toEqual([1])
    })

    it('treats slow taps as two single taps', () => {
      const s = setup()
      s.tap(4)
      s.advance(DOUBLE_TAP_MS)
      s.tap(4)
      expect(s.opened).toEqual([])
    })

    it('never pairs taps on different rows', () => {
      const s = setup()
      s.tap(4)
      s.advance(100)
      s.tap(5)
      expect(s.opened).toEqual([])
    })

    it('never pairs a tap on a retained row with one on its replacement', () => {
      const s = setup()
      s.tap(0)
      s.setListing('listing-2')
      s.advance(100)
      s.tap(0)
      expect(s.opened).toEqual([])
    })

    it('opens once on a triple tap', () => {
      const s = setup()
      s.tap(4)
      s.advance(100)
      s.tap(4)
      s.advance(100)
      s.tap(4)
      expect(s.opened).toEqual([4])
    })

    it('never pairs a mouse click with a tap', () => {
      const s = setup()
      s.tap(4, 'mouse')
      s.advance(100)
      s.tap(4)
      expect(s.opened).toEqual([])
    })
  })
})
