/**
 * Tier 3 a11y coverage for a server card's facts, on their own and inside a refused card.
 *
 * The rows are `<dt>`/`<dd>` pairs the group's `<dl>` holds, so each test mounts them into a
 * real `<dl>`: axe judges the list a screen reader walks. What these pin beyond axe: every row
 * is named by its label, an edit's arrow is hidden and spoken as words ("old changes to new"),
 * an unset value says so, and nothing on the card takes input.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount } from 'svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import type { ServerProposalView } from '#lib/ipc/bindings.js'

const { store } = vi.hoisted(() => ({
  store: { state: { busyGroupId: null as number | null, refusals: new Map<number, unknown>() } },
}))

// Only the refused-card test mounts the group, which reads its refusal and buttons from the store.
vi.mock('./suggested-ops-trigger.svelte', () => ({
  suggestedOpsState: store.state,
  approvableCount: () => 0,
  approveGroup: vi.fn(),
  canApprove: () => true,
  rejectGroup: vi.fn(),
  reloadGroup: vi.fn(),
  reviewOf: () => null,
}))

const SuggestedServerFacts = (await import('./SuggestedServerFacts.svelte')).default

const addServer: ServerProposalView = {
  protocol: 'sftp',
  fields: [
    { field: 'name', value: 'Build box', changed: false, previous: null },
    { field: 'address', value: 'build.local', changed: false, previous: null },
    { field: 'port', value: '22', changed: false, previous: null },
    { field: 'account', value: 'ada', changed: false, previous: null },
    { field: 'folder', value: '/srv/builds', changed: false, previous: null },
  ],
  addressIsNew: true,
  sharesAffected: null,
}

const editServer: ServerProposalView = {
  protocol: 'smb',
  fields: [
    { field: 'name', value: 'Naspolya', changed: true, previous: 'naspi' },
    { field: 'address', value: 'naspi.local', changed: false, previous: null },
    { field: 'account', value: 'david', changed: true, previous: null },
    { field: 'startFolder', value: null, changed: true, previous: '/media' },
  ],
  addressIsNew: false,
  sharesAffected: 3,
}

/** Mounts the facts into a `<dl>`, the parent the group gives them. */
function mountFacts(server: ServerProposalView): HTMLDListElement {
  const list = document.createElement('dl')
  document.body.appendChild(list)
  mount(SuggestedServerFacts, { target: list, props: { server } })
  flushSync()
  return list
}

/** Each row as a screen reader reads it: the label, then the value's text with hidden parts dropped. */
function spokenRows(list: HTMLElement): Array<[string, string]> {
  const terms = [...list.querySelectorAll('dt')]
  return terms.map((dt) => {
    const dd = dt.nextElementSibling as HTMLElement
    const clone = dd.cloneNode(true) as HTMLElement
    clone.querySelectorAll('[aria-hidden="true"]').forEach((hidden) => {
      hidden.remove()
    })
    return [dt.textContent.trim(), clone.textContent.replace(/\s+/g, ' ').trim()]
  })
}

beforeEach(() => {
  document.body.innerHTML = ''
  store.state.busyGroupId = null
  store.state.refusals = new Map<number, unknown>()
})

describe('an add card', () => {
  it('has no a11y violations', async () => {
    const list = mountFacts(addServer)

    await expectNoA11yViolations(list)
  })

  it('names every row by its label, the protocol first, and marks the new address', () => {
    const list = mountFacts(addServer)

    expect(spokenRows(list)).toEqual([
      ['Type', 'SFTP'],
      ['Name', 'Build box'],
      ['Address', 'build.local A new address'],
      ['Port', '22'],
      ['Account', 'ada'],
      ['Root folder', '/srv/builds'],
    ])
    expect(list.querySelector('.arrow')).toBeNull()
  })

  it('takes no input of any kind', () => {
    const list = mountFacts(addServer)

    expect(list.querySelectorAll('input, textarea, select, button, [contenteditable]')).toHaveLength(0)
  })
})

describe('an edit card', () => {
  it('has no a11y violations', async () => {
    const list = mountFacts(editServer)

    await expectNoA11yViolations(list)
  })

  // The arrow is a glyph, so it's hidden and the change is spoken as words beside it.
  it('speaks each changed row as old changes to new, and leaves the rest plain', () => {
    const list = mountFacts(editServer)

    expect(spokenRows(list)).toEqual([
      ['Type', 'SMB'],
      ['Name', 'naspi changes to Naspolya'],
      ['Address', 'naspi.local'],
      ['Account', 'Not set changes to david'],
      ['Start folder', '/media changes to Not set'],
    ])
    const arrows = [...list.querySelectorAll('.arrow')]
    expect(arrows).toHaveLength(3)
    for (const arrow of arrows) expect(arrow.getAttribute('aria-hidden')).toBe('true')
    expect(list.textContent).not.toContain('A new address')
  })
})

describe('a refused card', () => {
  it('keeps its facts readable beside the alert that says why, with no violations', async () => {
    store.state.refusals = new Map([[7, { kind: 'server', answer: { kind: 'nameTaken', name: 'Naspolya' } }]])
    const SuggestedOpsGroup = (await import('./SuggestedOpsGroup.svelte')).default
    const host = document.createElement('div')
    document.body.appendChild(host)
    mount(SuggestedOpsGroup, {
      target: host,
      props: {
        group: {
          groupId: 7,
          sweepId: 1,
          verb: 'addServer',
          status: 'pending',
          displayName: 'Build box',
          rationale: 'You SSH into this box every day.',
          reversible: 'serverSettings',
          runsAs: null,
          destination: null,
          destinationVolumeName: null,
          destinationState: 'notApplicable',
          sourceFolder: null,
          liveOpCount: 0,
          totalOpCount: 0,
          fromSelector: false,
          server: addServer,
        } as never,
        actionLabels: [],
      },
    })
    flushSync()

    const refusal = host.querySelector('.refusal')
    expect(refusal?.getAttribute('role')).toBe('alert')
    expect(refusal?.textContent).toContain('Another saved server is already called Naspolya')
    const facts = host.querySelector<HTMLElement>('dl.facts')
    expect(facts).not.toBeNull()
    expect(spokenRows(facts as HTMLElement)).toContainEqual(['Address', 'build.local A new address'])
    await expectNoA11yViolations(host)
  })
})
