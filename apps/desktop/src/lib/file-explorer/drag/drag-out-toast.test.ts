import { describe, it, expect } from 'vitest'
import { composeDragOutCompleteToast, type DragOutSessionComplete } from './drag-out-toast'

function payload(over: Partial<DragOutSessionComplete>): DragOutSessionComplete {
  return { sessionKey: 1, filesSucceeded: 0, foldersSucceeded: 0, failures: [], ...over }
}

describe('composeDragOutCompleteToast', () => {
  it('full success of one file reads "Copied 1 file." at success level', () => {
    const { message, level } = composeDragOutCompleteToast(payload({ filesSucceeded: 1 }))
    expect(message).toBe('Copied 1 file.')
    expect(level).toBe('success')
  })

  it('full success splits files and folders (selection-split contract)', () => {
    const { message, level } = composeDragOutCompleteToast(payload({ filesSucceeded: 2, foldersSucceeded: 1 }))
    expect(message).toBe('Copied 2 files and 1 folder.')
    expect(level).toBe('success')
  })

  it('full success of folders only reads the folder count', () => {
    const { message } = composeDragOutCompleteToast(payload({ foldersSucceeded: 3 }))
    expect(message).toBe('Copied 3 folders.')
  })

  it('partial success names a single failed file and warns', () => {
    const { message, level } = composeDragOutCompleteToast(payload({ filesSucceeded: 2, failures: ['video.mov'] }))
    expect(message).toBe("Copied 2 files, but couldn't copy video.mov.")
    expect(level).toBe('warn')
  })

  it('partial success collapses multiple failures to a count', () => {
    const { message, level } = composeDragOutCompleteToast(
      payload({ filesSucceeded: 1, failures: ['a.jpg', 'b.jpg', 'c.jpg'] }),
    )
    expect(message).toBe("Copied 1 file, but couldn't copy 3 files.")
    expect(level).toBe('warn')
  })

  it('total failure of one file names it at error level', () => {
    const { message, level } = composeDragOutCompleteToast(payload({ failures: ['clip.mov'] }))
    expect(message).toBe("Couldn't copy clip.mov.")
    expect(level).toBe('error')
  })

  it('total failure of several files reads a count at error level', () => {
    const { message, level } = composeDragOutCompleteToast(payload({ failures: ['a', 'b'] }))
    expect(message).toBe("Couldn't copy 2 files.")
    expect(level).toBe('error')
  })
})
