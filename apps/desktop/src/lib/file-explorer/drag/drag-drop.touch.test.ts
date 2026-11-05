/**
 * A drag a finger started (`touch-hold.ts` hands the held `pointerdown` to the view's mousedown
 * handler): a touch sends pointer events and no `mousemove`, so the tracking follows that pointer,
 * and the OS's long press or the browser taking the touch ends it.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'

const { dragged } = vi.hoisted(() => ({ dragged: [] as string[] }))

vi.mock('@tauri-apps/api/path', () => ({
  tempDir: () => Promise.resolve('/tmp'),
  join: (...parts: string[]) => Promise.resolve(parts.join('/')),
}))
vi.mock('@tauri-apps/plugin-fs', () => ({ writeFile: () => Promise.resolve() }))
vi.mock('#lib/icon-cache.js', () => ({ getCachedIcon: () => 'data:image/png;base64,AA==' }))
vi.mock('#lib/tauri-commands/index.js', () => ({
  startDragPaths: (paths: string[]) => {
    dragged.push(...paths)
    return Promise.resolve()
  },
  prepareSelfDragOverlay: () => Promise.resolve(),
  clearSelfDragOverlay: () => Promise.resolve(),
  setSelfDragResolvedOperation: () => Promise.resolve(),
}))
vi.mock('#lib/settings/settings-store.js', () => ({ getSetting: vi.fn(() => 5) }))
vi.mock('../rename/rename-activation', () => ({ cancelClickToRename: vi.fn() }))
vi.mock('./drag-image-renderer', () => ({ renderDragImage: () => Promise.reject(new Error('no canvas here')) }))

import { startSelectionDragTracking, cancelDragTracking } from './drag-drop'

const PATH = '/Users/you/notes.md'

function touch(type: string, init: PointerEventInit = {}): PointerEvent {
  return new PointerEvent(type, { pointerId: 3, pointerType: 'touch', isPrimary: true, button: 0, ...init })
}

/** Long enough for a started drag's async chain to reach `startDragPaths`. */
function settle(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 50))
}

function track(press: MouseEvent) {
  const onDragCancel = vi.fn()
  startSelectionDragTracking(
    press,
    { type: 'single', path: PATH, iconId: 'ext:md', index: 1, sourceVolumeId: 'root' },
    { onDragCancel },
  )
  return { onDragCancel }
}

describe('a drag started by a held touch', () => {
  beforeEach(() => {
    dragged.length = 0
    cancelDragTracking()
  })

  it('follows the finger past the threshold and drags', async () => {
    track(touch('pointerdown', { clientX: 0, clientY: 0 }))
    document.dispatchEvent(touch('pointermove', { clientX: 40, clientY: 0 }))
    await vi.waitFor(() => {
      expect(dragged).toEqual([PATH])
    })
  })

  it('ignores another pointer moving', async () => {
    track(touch('pointerdown', { clientX: 0, clientY: 0 }))
    document.dispatchEvent(touch('pointermove', { pointerId: 4, clientX: 40, clientY: 0 }))
    document.dispatchEvent(new MouseEvent('mousemove', { clientX: 40, clientY: 0 }))
    await settle()
    expect(dragged).toEqual([])
  })

  it('selects on a lift before the threshold, as a mouse release does', () => {
    const { onDragCancel } = track(touch('pointerdown'))
    document.dispatchEvent(touch('pointerup'))
    expect(onDragCancel).toHaveBeenCalledOnce()
  })

  it('ends when the browser takes the touch', () => {
    const { onDragCancel } = track(touch('pointerdown'))
    document.dispatchEvent(touch('pointercancel'))
    expect(onDragCancel).toHaveBeenCalledOnce()
  })

  it('hands a still finger to the OS long press: the context menu ends the tracking', async () => {
    const { onDragCancel } = track(touch('pointerdown', { clientX: 0, clientY: 0 }))
    document.dispatchEvent(new MouseEvent('contextmenu'))
    document.dispatchEvent(touch('pointermove', { clientX: 40, clientY: 0 }))
    await settle()
    expect(onDragCancel).toHaveBeenCalledOnce()
    expect(dragged).toEqual([])
  })
})

describe('a drag started by a mouse', () => {
  beforeEach(() => {
    dragged.length = 0
    cancelDragTracking()
  })

  it('still follows mousemove', async () => {
    track(new MouseEvent('mousedown', { clientX: 0, clientY: 0 }))
    document.dispatchEvent(new MouseEvent('mousemove', { clientX: 40, clientY: 0 }))
    await vi.waitFor(() => {
      expect(dragged).toEqual([PATH])
    })
  })

  it('ends on the mouse release a tap sends after its pointer events', () => {
    // A tap's compatibility mousedown arrives after its pointerup: only a mouseup ends it.
    const { onDragCancel } = track(new MouseEvent('mousedown'))
    document.dispatchEvent(new MouseEvent('mouseup'))
    expect(onDragCancel).toHaveBeenCalledOnce()
  })
})
