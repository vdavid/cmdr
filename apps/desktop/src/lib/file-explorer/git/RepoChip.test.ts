/**
 * Snapshot tests for the six visual states of `RepoChip.svelte`.
 *
 * The plan calls these "snapshot" tests; we use rendered text + data-state
 * attribute as the snapshot surface (DOM-string snapshots are noisy and
 * tend to fail on irrelevant whitespace).
 */
import { describe, it, expect } from 'vitest'
import { mount, tick } from 'svelte'
import RepoChip from './RepoChip.svelte'
import type { RepoInfo } from './git-store.svelte'

function base(): RepoInfo {
  return {
    repoRoot: '/repo',
    branch: 'main',
    detachedSha: null,
    unborn: false,
    upstream: null,
    ahead: null,
    behind: null,
    isDirty: false,
  }
}

function render(info: RepoInfo): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(RepoChip, { target, props: { info } })
  return target
}

describe('RepoChip', () => {
  it('renders clean state', async () => {
    const target = render(base())
    await tick()
    const chip = target.querySelector('.repo-chip')
    expect(chip).toBeTruthy()
    if (!chip) return
    expect(chip.getAttribute('data-state')).toBe('clean')
    expect(chip.textContent).toContain('main')
    expect(chip.textContent).not.toContain('dirty')
  })

  it('renders ahead state with +N', async () => {
    const target = render({ ...base(), upstream: 'origin/main', ahead: 3, behind: 0 })
    await tick()
    const chip = target.querySelector('.repo-chip')
    expect(chip).toBeTruthy()
    if (!chip) return
    expect(chip.getAttribute('data-state')).toBe('ahead')
    expect(chip.textContent).toContain('+3')
  })

  it('renders behind state with -N', async () => {
    const target = render({ ...base(), upstream: 'origin/main', ahead: 0, behind: 2 })
    await tick()
    const chip = target.querySelector('.repo-chip')
    expect(chip).toBeTruthy()
    if (!chip) return
    expect(chip.getAttribute('data-state')).toBe('behind')
    expect(chip.textContent).toContain('-2')
  })

  it('renders dirty state', async () => {
    const target = render({ ...base(), isDirty: true })
    await tick()
    const chip = target.querySelector('.repo-chip')
    expect(chip).toBeTruthy()
    if (!chip) return
    expect(chip.getAttribute('data-state')).toBe('dirty')
    expect(chip.textContent).toContain('dirty')
  })

  it('renders detached state with short sha', async () => {
    const target = render({ ...base(), branch: null, detachedSha: 'a1b2c3d' })
    await tick()
    const chip = target.querySelector('.repo-chip')
    expect(chip).toBeTruthy()
    if (!chip) return
    expect(chip.getAttribute('data-state')).toBe('detached')
    expect(chip.textContent).toContain('a1b2c3d')
    expect(chip.textContent).toContain('(detached)')
  })

  it('renders unborn state', async () => {
    const target = render({ ...base(), unborn: true })
    await tick()
    const chip = target.querySelector('.repo-chip')
    expect(chip).toBeTruthy()
    if (!chip) return
    expect(chip.getAttribute('data-state')).toBe('unborn')
    expect(chip.textContent).toContain('main')
    expect(chip.textContent).toContain('no commits yet')
  })

  it('aria-label carries the full status sentence', async () => {
    const target = render({ ...base(), upstream: 'origin/main', ahead: 1, behind: 0, isDirty: true })
    await tick()
    const chip = target.querySelector('.repo-chip') as HTMLElement
    const aria = chip.getAttribute('aria-label') ?? ''
    expect(aria).toContain('On branch main')
    expect(aria).toContain('1 ahead')
    expect(aria).toContain('uncommitted')
  })
})
