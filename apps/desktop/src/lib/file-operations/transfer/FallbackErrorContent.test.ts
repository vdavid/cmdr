/**
 * Behavioral tests for `FallbackErrorContent.svelte`.
 *
 * The component renders the variant-derived message + suggestion for a typed
 * `WriteOperationError`, and for the `files_too_large_for_filesystem` (FAT32
 * 4 GiB cap) variant it additionally lists the offending files and an "and N
 * more" line when the backend capped the list. These tests render the real
 * component (real i18n catalog + `formatBytes`) across those branches.
 */

import { describe, expect, it } from 'vitest'
import { mount, tick } from 'svelte'
import FallbackErrorContent from './FallbackErrorContent.svelte'
import type { WriteOperationError } from '$lib/file-explorer/types'

function mountFallback(error: WriteOperationError, operationType: 'copy' | 'move' | 'delete' | 'trash' = 'copy') {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(FallbackErrorContent, { target, props: { error, operationType } })
  return target
}

describe('FallbackErrorContent', () => {
  it('renders message and suggestion for a non-oversized variant, with no file list', async () => {
    const target = mountFallback({ type: 'permission_denied', path: '/Users/test/protected.txt', message: 'EACCES' })
    await tick()

    expect(target.querySelector('#error-dialog-message')?.textContent).toBeTruthy()
    expect(target.querySelector('.suggestion')?.textContent.trim()).toBeTruthy()
    // The oversized-files list only appears for files_too_large_for_filesystem.
    expect(target.querySelector('.oversized-files')).toBeNull()
    expect(target.querySelector('.oversized-more')).toBeNull()
  })

  it('lists the single offending file for a one-file FAT32 too-large error', async () => {
    const target = mountFallback({
      type: 'files_too_large_for_filesystem',
      filesystem: 'fat32',
      maxSize: 4 * 1024 * 1024 * 1024,
      totalCount: 1,
      files: [{ name: 'movie.mkv', size: 5 * 1024 * 1024 * 1024 }],
    })
    await tick()

    const items = target.querySelectorAll('.oversized-files li')
    expect(items).toHaveLength(1)
    expect(items[0].querySelector('.file-name')?.textContent).toBe('movie.mkv')
    expect(items[0].querySelector('.file-size')?.textContent).toContain('5.0 GB')
    // One file exactly: nothing is hidden, so no "and N more" line.
    expect(target.querySelector('.oversized-more')).toBeNull()
  })

  it('lists every offending file when none are hidden', async () => {
    const target = mountFallback(
      {
        type: 'files_too_large_for_filesystem',
        filesystem: 'fat32',
        maxSize: 4 * 1024 * 1024 * 1024,
        totalCount: 2,
        files: [
          { name: 'a.iso', size: 6 * 1024 * 1024 * 1024 },
          { name: 'b.iso', size: 7 * 1024 * 1024 * 1024 },
        ],
      },
      'move',
    )
    await tick()

    const items = target.querySelectorAll('.oversized-files li')
    expect(items).toHaveLength(2)
    expect(Array.from(items, (li) => li.querySelector('.file-name')?.textContent)).toEqual(['a.iso', 'b.iso'])
    expect(target.querySelector('.oversized-more')).toBeNull()
  })

  it('shows an "and N more" line when the file list is capped', async () => {
    const target = mountFallback({
      type: 'files_too_large_for_filesystem',
      filesystem: 'fat32',
      maxSize: 4 * 1024 * 1024 * 1024,
      totalCount: 12,
      files: [
        { name: 'one.iso', size: 5 * 1024 * 1024 * 1024 },
        { name: 'two.iso', size: 5 * 1024 * 1024 * 1024 },
      ],
    })
    await tick()

    expect(target.querySelectorAll('.oversized-files li')).toHaveLength(2)
    const more = target.querySelector('.oversized-more')
    expect(more?.textContent).toContain('10')
    expect(more?.textContent.toLowerCase()).toContain('more')
  })
})
