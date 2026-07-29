/**
 * The global right-click decision: a text field keeps WebKit's own editing menu
 * (Cut / Copy / Paste / Select All), everything else keeps Cmdr's menus only.
 *
 * The load-bearing case is the TARGET, not focus: a right-click lands on a field
 * that isn't focused yet, so `isTextInputFocused()` would say "no" and the field
 * would lose its menu.
 */
import { describe, it, expect } from 'vitest'
import { resolveGlobalContextMenuAction } from './global-contextmenu'

/** A right-click whose target is `el`, as the document-level listener sees it. */
function rightClickOn(el: Element): MouseEvent {
  document.body.appendChild(el)
  const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true })
  el.dispatchEvent(event)
  el.remove()
  return event
}

describe('resolveGlobalContextMenuAction', () => {
  it('hands an <input> to the native text menu', () => {
    expect(resolveGlobalContextMenuAction(rightClickOn(document.createElement('input')))).toBe('native-text-menu')
  })

  it('hands a <textarea> to the native text menu', () => {
    expect(resolveGlobalContextMenuAction(rightClickOn(document.createElement('textarea')))).toBe('native-text-menu')
  })

  it('hands a contenteditable host to the native text menu', () => {
    const host = document.createElement('div')
    host.contentEditable = 'true'
    expect(resolveGlobalContextMenuAction(rightClickOn(host))).toBe('native-text-menu')
  })

  it('hands an element INSIDE a contenteditable host to the native text menu', () => {
    const host = document.createElement('div')
    host.contentEditable = 'true'
    const inner = document.createElement('span')
    host.appendChild(inner)
    document.body.appendChild(host)
    const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true })
    inner.dispatchEvent(event)
    host.remove()
    expect(resolveGlobalContextMenuAction(event)).toBe('native-text-menu')
  })

  it('hands the rename editor inside a file row to the native text menu', () => {
    const row = document.createElement('div')
    row.className = 'file-row'
    const input = document.createElement('input')
    row.appendChild(input)
    document.body.appendChild(row)
    const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true })
    input.dispatchEvent(event)
    row.remove()
    expect(resolveGlobalContextMenuAction(event)).toBe('native-text-menu')
  })

  it("suppresses on a file row, so only Cmdr's own menu opens", () => {
    const row = document.createElement('div')
    row.className = 'file-row'
    expect(resolveGlobalContextMenuAction(rightClickOn(row))).toBe('suppress')
  })

  it('suppresses on a button', () => {
    expect(resolveGlobalContextMenuAction(rightClickOn(document.createElement('button')))).toBe('suppress')
  })

  it('suppresses a targetless event', () => {
    expect(resolveGlobalContextMenuAction(new MouseEvent('contextmenu'))).toBe('suppress')
  })
})
