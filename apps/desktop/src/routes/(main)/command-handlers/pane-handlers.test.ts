/**
 * Behavior tests for `pane.clone`: the other pane opens the focused pane's
 * location exactly (the cursor never refines it), whichever pane is focused.
 */
import { describe, it, expect, vi } from 'vitest'

import { paneHandlers } from './pane-handlers'
import type { CommandHandlerContext } from './types'

function runClone(focused: 'left' | 'right') {
  const copyPathBetweenPanes = vi.fn()
  const explorerRef = { getFocusedPane: () => focused, copyPathBetweenPanes }
  const hctx = { explorerRef, ctx: {}, dispatchArgs: undefined } as unknown as CommandHandlerContext
  paneHandlers['pane.clone'](hctx)
  return copyPathBetweenPanes
}

describe('pane.clone handler', () => {
  it('clones the focused left pane into the right one, ignoring the cursor', () => {
    expect(runClone('left')).toHaveBeenCalledWith({ source: 'left', target: 'right', followCursor: false })
  })

  it('clones the focused right pane into the left one', () => {
    expect(runClone('right')).toHaveBeenCalledWith({ source: 'right', target: 'left', followCursor: false })
  })

  it('does nothing without an explorer', () => {
    const hctx = { explorerRef: undefined, ctx: {}, dispatchArgs: undefined } as unknown as CommandHandlerContext
    expect(() => {
      paneHandlers['pane.clone'](hctx)
    }).not.toThrow()
  })
})
