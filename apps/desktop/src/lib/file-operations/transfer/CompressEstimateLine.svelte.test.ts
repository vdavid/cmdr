/**
 * Component tests for `CompressEstimateLine.svelte`: the render, live re-scale,
 * loading, and absent states of the Compress dialog's estimated-size line.
 * Kept out of `TransferDialog.test.ts` (line-budgeted).
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushSync, type ComponentProps } from 'svelte'

// The component reads the compression-level setting and subscribes to its
// changes; drive both from the test while preserving every other `$lib/settings`
// export (intl and `<Size>` pull from the same module).
let currentLevel = 6
let changeCb: ((id: string, v: unknown) => void) | null = null
vi.mock('$lib/settings', async (importOriginal) => ({
  ...(await importOriginal<typeof import('$lib/settings')>()),
  getSetting: () => currentLevel,
  onSpecificSettingChange: (_id: string, cb: (id: string, v: unknown) => void) => {
    changeCb = cb
    return () => {}
  },
}))
vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getFileSizeFormat: () => 'decimal',
}))

import CompressEstimateLine from './CompressEstimateLine.svelte'
import type { CompressedSizeEstimate } from '$lib/tauri-commands'

let target: HTMLElement
function render(props: ComponentProps<typeof CompressEstimateLine>) {
  target = document.createElement('div')
  document.body.appendChild(target)
  mount(CompressEstimateLine, { target, props })
  flushSync()
}

beforeEach(() => {
  document.body.innerHTML = ''
  currentLevel = 6
  changeCb = null
})

// Compressible content only, so a level change visibly moves the number.
const estimate: CompressedSizeEstimate = { compressibleBytes: 1_000_000, mediumBytes: 0, incompressibleBytes: 0 }

describe('CompressEstimateLine', () => {
  it('renders an explicitly-approximate size when the estimate is present', () => {
    render({ estimate, isScanning: false, sourceIsLocal: true })
    expect(target.textContent).toContain('Estimated size')
    expect(target.textContent).toContain('~')
    expect(target.querySelector('.estimate-value')).not.toBeNull()
  })

  it('re-scales the shown value live when the compression level changes', () => {
    render({ estimate, isScanning: false, sourceIsLocal: true })
    const atLevelSix = target.querySelector('.estimate-value')?.textContent ?? ''
    // Slide to the "Faster" end (level 1): compressible content inflates ~45%.
    currentLevel = 1
    changeCb?.('behavior.archiveCompressionLevel', 1)
    flushSync()
    const atLevelOne = target.querySelector('.estimate-value')?.textContent ?? ''
    expect(atLevelOne).not.toBe(atLevelSix)
  })

  it('shows a loading affordance while a local scan runs with no estimate yet', () => {
    render({ estimate: null, isScanning: true, sourceIsLocal: true })
    expect(target.textContent).toContain('Estimated size')
    expect(target.textContent).not.toContain('~')
    expect(target.querySelector('.estimate-value')).toBeNull()
  })

  it('renders nothing for a remote source (never sampled), even while scanning', () => {
    render({ estimate: null, isScanning: true, sourceIsLocal: false })
    expect(target.textContent.trim()).toBe('')
  })

  it('renders nothing once a local scan completes with no estimate', () => {
    render({ estimate: null, isScanning: false, sourceIsLocal: true })
    expect(target.textContent.trim()).toBe('')
  })
})
