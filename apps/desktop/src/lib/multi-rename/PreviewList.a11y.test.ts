/**
 * Tier 3 a11y test for `PreviewList.svelte`: a ready row, an unchanged one, a row whose
 * new name was typed in Results (the pencil where the arrow is), and a problem row.
 */

import { describe, it, expect } from 'vitest'
import { mount, tick } from 'svelte'
import PreviewList from './PreviewList.svelte'
import type { PreviewRow } from '$lib/tauri-commands'
import { expectNoA11yViolations } from '$lib/test-a11y'

const ROWS: PreviewRow[] = [
  {
    row: 0,
    oldName: 'a.pdf',
    newName: 'b.pdf',
    status: { type: 'ready' },
    iconId: null,
    isDirectory: false,
    edited: false,
  },
  {
    row: 1,
    oldName: 'c.pdf',
    newName: 'c.pdf',
    status: { type: 'unchanged' },
    iconId: null,
    isDirectory: false,
    edited: false,
  },
  {
    row: 2,
    oldName: 'd.pdf',
    newName: 'dee.pdf',
    status: { type: 'ready' },
    iconId: null,
    isDirectory: false,
    edited: true,
  },
  {
    row: 3,
    oldName: 'e.pdf',
    newName: 'b.pdf',
    status: { type: 'duplicate' },
    iconId: null,
    isDirectory: false,
    edited: true,
  },
]

describe('PreviewList a11y', () => {
  it('with ready, unchanged, typed, and problem rows has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(PreviewList, {
      target,
      props: { rows: { count: ROWS.length, getRow: (index: number) => ROWS[index], onRangeChange: () => {} } },
    })
    await tick()
    expect(target.querySelectorAll('.edited-mark')).toHaveLength(2)
    await expectNoA11yViolations(target)
  })
})
