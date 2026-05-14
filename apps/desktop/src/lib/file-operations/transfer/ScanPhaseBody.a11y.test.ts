/**
 * Tier 3 a11y tests for `ScanPhaseBody.svelte`.
 *
 * Pure presentational component rendered inside `TransferProgressDialog`
 * during the scan phase. Shows source path, running tallies (bytes / files /
 * dirs), optional progress bar against drive-index totals, throughput, and
 * current dir / current file. No Tauri deps; data flows in via props.
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import ScanPhaseBody from './ScanPhaseBody.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('ScanPhaseBody a11y', () => {
  it('early scan state (no totals, no throughput, no current file) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ScanPhaseBody, {
      target,
      props: {
        sourceFolderPath: '/Users/test/documents',
        scanFilesFound: 0,
        scanDirsFound: 0,
        scanBytesFound: 0,
        scanExpectedFiles: null,
        scanProgressFraction: null,
        scanFilesPerSec: null,
        scanBytesPerSec: null,
        scanCurrentDir: null,
        currentFile: null,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('mid-scan state (progress bar + throughput + current dir + file) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ScanPhaseBody, {
      target,
      props: {
        sourceFolderPath: '/Users/test/documents',
        scanFilesFound: 1234,
        scanDirsFound: 56,
        scanBytesFound: 5_678_901_234,
        scanExpectedFiles: 5000,
        scanProgressFraction: 0.42,
        scanFilesPerSec: 850,
        scanBytesPerSec: 12_345_678,
        scanCurrentDir: '/Users/test/documents/projects/cmdr/apps/desktop/src',
        currentFile: 'large-build-artifact.tar.gz',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('long paths (shorten-middle action) have no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ScanPhaseBody, {
      target,
      props: {
        sourceFolderPath: '/Volumes/External/very/deeply/nested/folder/structure/with/many/levels/of/depth',
        scanFilesFound: 1,
        scanDirsFound: 1,
        scanBytesFound: 1024,
        scanExpectedFiles: null,
        scanProgressFraction: null,
        scanFilesPerSec: null,
        scanBytesPerSec: null,
        scanCurrentDir: '/Volumes/External/very/deeply/nested/folder/structure/with/many/levels/of/depth/subdir',
        currentFile: 'a-file-with-a-rather-long-name-that-exceeds-the-container-width.txt',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
