import { describe, expect, it, vi } from 'vitest'
import { type MenuClickDeps, performMenuClick } from './menu-click.js'

function deps(front?: { label: string; frame: HTMLIFrameElement }) {
  const emitted: unknown[][] = []
  const invoke = vi.fn(() => Promise.resolve(null))
  const close = vi.fn()
  const value: MenuClickDeps = {
    emit: (...args) => emitted.push(args),
    invoke,
    windows: { front: () => front, close },
  }
  return { value, emitted, invoke, close }
}

describe('performMenuClick', () => {
  it('emits to the window the pick names', () => {
    const d = deps()
    performMenuClick(
      { kind: 'emit', event: 'queue-row-context-action', payload: { action: 'cancel' }, to: 'queue' },
      d.value,
    )
    expect(d.emitted).toEqual([['queue-row-context-action', { action: 'cancel' }, { kind: 'label', label: 'queue' }]])
  })

  it('runs a command pick, and a refusal only warns', async () => {
    const d = deps()
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    d.invoke.mockReturnValueOnce(Promise.reject(new Error('refused')))
    performMenuClick({ kind: 'invoke', command: 'add_favorite', args: { path: '/a' } }, d.value)
    expect(d.invoke).toHaveBeenCalledWith('add_favorite', { path: '/a' })
    await Promise.resolve()
    await Promise.resolve()
    expect(warn).toHaveBeenCalled()
    warn.mockRestore()
  })

  it('hands a native edit to the window in front, and does nothing with none', () => {
    // The test DOM loads no iframe page, so a stand-in with the two members the pick touches.
    const exec = vi.fn(() => true)
    const focus = vi.fn()
    const frame = { focus, contentDocument: { execCommand: exec } } as unknown as HTMLIFrameElement
    performMenuClick({ kind: 'nativeEdit', action: 'copy' }, deps({ label: 'settings', frame }).value)
    expect(focus).toHaveBeenCalled()
    expect(exec).toHaveBeenCalledWith('copy')
    expect(() => {
      performMenuClick({ kind: 'nativeEdit', action: 'copy' }, deps().value)
    }).not.toThrow()
  })

  it('closes the window in front, and nothing does nothing', () => {
    const frame = document.createElement('iframe')
    const d = deps({ label: 'settings', frame })
    performMenuClick({ kind: 'closeFocusedWindow' }, d.value)
    expect(d.close).toHaveBeenCalledWith('settings')
    const none = deps()
    performMenuClick({ kind: 'closeFocusedWindow' }, none.value)
    performMenuClick({ kind: 'nothing' }, none.value)
    expect(none.close).not.toHaveBeenCalled()
    expect(none.emitted).toEqual([])
  })
})
