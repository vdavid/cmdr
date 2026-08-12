import { describe, expect, it } from 'vitest'
import { indexIsSettled, parsePaneTabs, parsePaneView } from './marketing-shots-state.js'

const STATE = `generation: 41
focused: left
showHidden: true
left:
  tabs:
    - i:0 id:t-1 [active] lib (/Users/d/repo/apps/desktop/src/lib)
  volume: Macintosh HD
  path: /Users/d/repo/apps/desktop/src/lib
  view: full
right:
  tabs:
    - i:0 id:t-7 [pinned] lib (/Users/d/repo/apps/desktop/src/lib)
    - i:1 id:t-8 [active] src (/Users/d/repo/apps/desktop/src-tauri/src)
    - i:2 id:t-9 [pinned] brand (/Users/d/repo/brand)
  volume: Macintosh HD
  path: /Users/d/repo/apps/desktop/src-tauri/src
  view: brief
volumes:
  - name: Macintosh HD
    indexStatus: fresh
  - name: naspi
    indexStatus: off
`

describe('parsePaneTabs', () => {
  it('reads a pane’s tabs without picking up the other pane’s', () => {
    // The two panes' tab lists are indistinguishable line by line, so a parser that
    // scans the whole document happily unpins the wrong pane's tab.
    expect(parsePaneTabs(STATE, 'left')).toEqual([{ id: 't-1', pinned: false }])
    expect(parsePaneTabs(STATE, 'right')).toEqual([
      { id: 't-7', pinned: true },
      { id: 't-8', pinned: false },
      { id: 't-9', pinned: true },
    ])
  })

  it('returns nothing for a pane with no synced tabs', () => {
    expect(parsePaneTabs('generation: 1\nfocused: left\nleft:\n  path: /\nright:\n  path: /\n', 'left')).toEqual([])
  })
})

describe('parsePaneView', () => {
  it('reads each pane’s view mode from its own block', () => {
    // Scoped like `parsePaneTabs`: both panes print an identical `view:` line, so a
    // document-wide scan reads the left pane's mode and skips the right pane's switch.
    expect(parsePaneView(STATE, 'left')).toBe('full')
    expect(parsePaneView(STATE, 'right')).toBe('brief')
  })

  it('returns null when the pane has no view line', () => {
    // A null must mean "don't know", never a guess: the caller skips a no-op
    // `set_view_mode`, and the backend only acks a call that actually changes something.
    expect(parsePaneView('generation: 1\nleft:\n  path: /\nright:\n  path: /\n', 'left')).toBeNull()
  })
})

describe('indexIsSettled', () => {
  it('accepts a state where nothing is scanning', () => {
    expect(indexIsSettled(STATE)).toBe(true)
  })

  it('rejects a state where any volume is still scanning', () => {
    // The whole point: while a drive scans, every folder size renders as an hourglass,
    // which is a full round of unusable masters.
    expect(indexIsSettled(STATE.replace('indexStatus: fresh', 'indexStatus: scanning'))).toBe(false)
  })

  it('rejects a state with no indexed volume at all', () => {
    expect(indexIsSettled('generation: 1\nvolumes:\n  - name: Macintosh HD\n    indexStatus: off\n')).toBe(false)
  })
})
