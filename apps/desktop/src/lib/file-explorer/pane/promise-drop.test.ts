import { describe, expect, it } from 'vitest'
import {
  canAcceptDrag,
  createPromiseDropPairing,
  dropSourceLabel,
  emptyDropOutcome,
  hostOf,
  promiseDropFailureMessage,
} from './promise-drop'

const chrome = { appName: 'Google Chrome', pageUrl: 'https://www.example.com/pics/cat.jpg' }

describe('canAcceptDrag', () => {
  it('lights targets up for files, promises, image bytes, and our own drags', () => {
    for (const kind of ['fileUrls', 'promises', 'imageData', 'inApp'] as const) {
      expect(canAcceptDrag(kind)).toBe(true)
    }
  })

  it('refuses a drag that carries nothing droppable', () => {
    expect(canAcceptDrag('nothing')).toBe(false)
  })

  it('keeps the old behavior when the native side never said (not macOS)', () => {
    expect(canAcceptDrag(null)).toBe(true)
  })
})

describe('emptyDropOutcome', () => {
  it('waits for a promise or image drop', () => {
    expect(emptyDropOutcome('promises')).toBe('promise')
    expect(emptyDropOutcome('imageData')).toBe('promise')
  })

  it('never counts an empty drop of anything else as a transfer', () => {
    expect(emptyDropOutcome('nothing')).toBe('nothingUsable')
    expect(emptyDropOutcome('fileUrls')).toBe('nothingUsable')
    expect(emptyDropOutcome(null)).toBe('nothingUsable')
  })
})

describe('hostOf', () => {
  it('reads the host without www', () => {
    expect(hostOf('https://www.example.com/a.jpg')).toBe('example.com')
    expect(hostOf('https://images.example.org:8080/a.jpg')).toBe('images.example.org')
  })

  it('has no host for data, file, and broken URLs', () => {
    expect(hostOf('data:image/png;base64,AAAA')).toBeNull()
    expect(hostOf('file:///Users/me/a.png')).toBeNull()
    expect(hostOf('not a url')).toBeNull()
    expect(hostOf(null)).toBeNull()
  })
})

describe('dropSourceLabel', () => {
  const one = ['/data/drops/drop-1-0/cat.jpg']
  const three = ['/d/a.jpg', '/d/b.jpg', '/d/c.jpg']

  it('names one file with its app and host', () => {
    expect(dropSourceLabel(chrome, one)).toBe('cat.jpg from Google Chrome (example.com)')
  })

  it('names one file with whatever it knows', () => {
    expect(dropSourceLabel({ appName: 'Mail', pageUrl: null }, one)).toBe('cat.jpg from Mail')
    expect(dropSourceLabel({ appName: null, pageUrl: chrome.pageUrl }, one)).toBe('cat.jpg from example.com')
    expect(dropSourceLabel({ appName: null, pageUrl: null }, one)).toBe('cat.jpg')
  })

  it('counts several files', () => {
    expect(dropSourceLabel(chrome, three)).toBe('3 files from Google Chrome (example.com)')
    expect(dropSourceLabel({ appName: 'Mail', pageUrl: null }, three)).toBe('3 files from Mail')
    expect(dropSourceLabel({ appName: null, pageUrl: null }, three)).toBe('3 dropped files')
  })

  it('never shows the drop folder', () => {
    expect(dropSourceLabel(chrome, one)).not.toContain('/')
  })
})

describe('promiseDropFailureMessage', () => {
  it('names the app when it knows it', () => {
    expect(promiseDropFailureMessage('nothingReceived', 'Google Chrome')).toBe(
      'Google Chrome didn’t hand over the file. Try dragging it again, or save it from Google Chrome first.',
    )
    expect(promiseDropFailureMessage('nothingReceived', null)).toContain('The app you dragged from')
  })
})

describe('createPromiseDropPairing', () => {
  const target = { pane: 'right' as const, folderPath: '/Users/me/Pictures' }
  const started = { dropId: 'drop-1-0', itemCount: 1, source: chrome }
  const ready = { dropId: 'drop-1-0', paths: ['/d/cat.jpg'], failedCount: 0 }

  it('opens once the drop and the files are both in, started first (the usual order)', () => {
    const pairing = createPromiseDropPairing()
    expect(pairing.started(started)).toEqual([])
    expect(pairing.dropped(target)).toEqual([{ type: 'receiving', dropId: 'drop-1-0', itemCount: 1, source: chrome }])
    expect(pairing.ready(ready)).toEqual([
      { type: 'open', dropId: 'drop-1-0', target, paths: ['/d/cat.jpg'], failedCount: 0, source: chrome },
    ])
  })

  it('pairs in any order', () => {
    const pairing = createPromiseDropPairing()
    expect(pairing.dropped(target)).toEqual([])
    expect(pairing.started(started)).toEqual([{ type: 'receiving', dropId: 'drop-1-0', itemCount: 1, source: chrome }])
    expect(pairing.ready(ready)[0]?.type).toBe('open')
  })

  it('holds early files until the drop says where they go', () => {
    const pairing = createPromiseDropPairing()
    pairing.started(started)
    expect(pairing.ready(ready)).toEqual([])
    expect(pairing.dropped(target).map((a) => a.type)).toEqual(['receiving', 'open'])
  })

  it('releases the files of a drop that hit no target', () => {
    const pairing = createPromiseDropPairing()
    pairing.started(started)
    expect(pairing.dropped(null)).toEqual([])
    expect(pairing.ready(ready)).toEqual([{ type: 'release', dropId: 'drop-1-0' }])
  })

  it('says why a drop produced nothing, with the app it came from', () => {
    const pairing = createPromiseDropPairing()
    pairing.started(started)
    pairing.dropped(target)
    expect(pairing.failed({ dropId: 'drop-1-0', reason: 'nothingReceived' })).toEqual([
      { type: 'failed', dropId: 'drop-1-0', reason: 'nothingReceived', appName: 'Google Chrome' },
    ])
  })

  it('pairs a failure that never started (no drop folder) with the waiting drop', () => {
    const pairing = createPromiseDropPairing()
    pairing.dropped(target)
    expect(pairing.failed({ dropId: 'drop-unavailable', reason: 'noDropFolder' })).toEqual([
      { type: 'failed', dropId: 'drop-unavailable', reason: 'noDropFolder', appName: null },
    ])
  })

  it('releases files nobody is waiting for', () => {
    const pairing = createPromiseDropPairing()
    expect(pairing.ready({ ...ready, dropId: 'drop-9-9' })).toEqual([{ type: 'release', dropId: 'drop-9-9' }])
  })

  it('keeps a slow drop going while the next drag starts', () => {
    const pairing = createPromiseDropPairing()
    pairing.started(started)
    pairing.dropped(target)
    pairing.newDrag()
    expect(pairing.ready(ready)[0]?.type).toBe('open')
  })

  it('forgets an orphaned drop target when the next drag starts', () => {
    const pairing = createPromiseDropPairing()
    pairing.dropped(target)
    pairing.newDrag()
    // It waits for its own drop, not the orphaned target.
    expect(pairing.started(started)).toEqual([])
  })

  it('releases the files of a drop whose drop event never came', () => {
    const pairing = createPromiseDropPairing()
    pairing.started(started)
    pairing.newDrag()
    expect(pairing.ready(ready)).toEqual([{ type: 'release', dropId: 'drop-1-0' }])
  })
})
