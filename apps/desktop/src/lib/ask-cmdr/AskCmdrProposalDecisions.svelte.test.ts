/**
 * The line recording what the user answered when the agent suggested something.
 *
 * The load-bearing property is the same one the wake digest has: every WORD comes from the
 * catalog and every NUMBER and PATH from the wire. The backend hands over a verb token, a
 * count, and the group's own display text precisely so ten locales can each say it their own
 * way, and a rendered backend sentence appearing here would be untranslated copy frozen in
 * `main.db`.
 *
 * The other half is honesty about approvals: an approved group can go on to skip every file,
 * so the result line always renders, zeros included.
 */

import { beforeEach, describe, expect, it } from 'vitest'
import { mount, tick } from 'svelte'
import AskCmdrProposalDecisions from './AskCmdrProposalDecisions.svelte'
import type { ProposalDecision } from '#lib/tauri-commands/index.js'
import { withPlatform } from '#lib/platform/test-platform.js'

function decision(overrides: Partial<ProposalDecision> = {}): ProposalDecision {
  return {
    verb: 'trash',
    what: '/Users/dana/Downloads/*.dmg',
    ops: 12,
    outcome: { kind: 'rejected' },
    ...overrides,
  }
}

function render(decisions: ProposalDecision[]): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AskCmdrProposalDecisions, { target, props: { decisions } })
  return target
}

describe('AskCmdrProposalDecisions', () => {
  it('says what the user turned down, in the catalog is words and the wire is numbers', async () => {
    const target = render([decision()])
    await tick()

    expect(target.textContent).toContain('You turned down trashing 12 items')
    expect(target.textContent).toContain('/Users/dana/Downloads/*.dmg')
  })

  /** Windows names the place, and English needs it after the count ("moving 3 items to the
   *  Recycle Bin"), so the trash line is a whole sentence of its own there, never a verb slot. */
  describe.each([
    {
      platform: 'macos' as const,
      rejected: 'You turned down trashing 12 items',
      approved: 'You approved trashing 1 item',
    },
    {
      platform: 'windows' as const,
      rejected: 'You turned down moving 12 items to the Recycle Bin',
      approved: 'You approved moving 1 item to the Recycle Bin',
    },
    {
      platform: 'linux' as const,
      rejected: 'You turned down trashing 12 items',
      approved: 'You approved trashing 1 item',
    },
  ])('a trash suggestion on $platform', ({ platform, rejected, approved }) => {
    beforeEach(() => withPlatform(platform))

    it('reads as one natural sentence', async () => {
      const target = render([
        decision(),
        decision({ ops: 1, outcome: { kind: 'ran', done: 1, skipped: 0, failed: 0 } }),
      ])
      await tick()

      expect(target.textContent).toContain(rejected)
      expect(target.textContent).toContain(approved)
    })

    it('keeps every other verb in the shared sentence', async () => {
      const target = render([decision({ verb: 'move', ops: 3 })])
      await tick()

      expect(target.textContent).toContain('You turned down moving 3 items')
      expect(target.textContent).not.toContain('Recycle Bin')
    })
  })

  it('names the verb the user actually answered about', async () => {
    const target = render([decision({ verb: 'delete', ops: 1 })])
    await tick()

    expect(target.textContent).toContain('You turned down deleting 1 item')
  })

  /** ⚠️ An approval is a claim; this line is what happened. A group can be approved and then
   *  skip every file behind a fingerprint mismatch, and hiding that would leave the user
   *  believing their files moved. */
  it('reports what an approved run actually did, zeros included', async () => {
    const target = render([
      decision({ verb: 'move', ops: 3, outcome: { kind: 'ran', done: 2, skipped: 1, failed: 0 } }),
    ])
    await tick()

    expect(target.textContent).toContain('You approved moving 3 items')
    expect(target.textContent).toContain('2 done, 1 skipped, 0 failed')
  })

  /** #345: the line appears the moment the user approves, long before a big copy settles. */
  it('says an approved run is still going', async () => {
    const target = render([decision({ verb: 'copy', ops: 1_204, outcome: { kind: 'running' } })])
    await tick()

    expect(target.textContent).toContain('You approved copying 1,204 items')
    expect(target.textContent).toContain('Running…')
  })

  /** The app quit before the run settled. The next launch says so, never "running…" forever. */
  it('says when the app quit before an approved run finished', async () => {
    const target = render([decision({ verb: 'copy', ops: 3, outcome: { kind: 'interrupted' } })])
    await tick()

    expect(target.textContent).toContain('You approved copying 3 items')
    expect(target.textContent).toContain('Cmdr quit before this finished')
    expect(target.textContent).not.toContain('Running')
  })

  /** A server suggestion has no files, so its line names the server and no count. */
  it('says a server suggestion was saved, naming no count', async () => {
    const target = render([
      decision({ verb: 'addServer', what: 'build-box', ops: 0, outcome: { kind: 'saved', note: null } }),
    ])
    await tick()

    expect(target.textContent).toContain('You approved adding a server')
    expect(target.textContent).toContain('build-box')
    expect(target.textContent).toContain('Saved')
    expect(target.textContent).not.toContain('0 items')
  })

  /** A save that wrote the host but not its share says so, so the user doesn't look for it. */
  it('says when a saved server is missing its share', async () => {
    const target = render([
      decision({ verb: 'addServer', what: 'naspolya', ops: 0, outcome: { kind: 'saved', note: 'shareNotSaved' } }),
    ])
    await tick()

    expect(target.textContent).toContain('Saved the server, not its share')
  })

  it('says what server change the user turned down', async () => {
    const target = render([decision({ verb: 'editServer', what: 'naspolya', ops: 0 })])
    await tick()

    expect(target.textContent).toContain('You turned down changing a server’s settings')
  })

  /** A rejection never ran, so there is nothing to report about it. */
  it('says nothing about a run for a suggestion that was turned down', async () => {
    const target = render([decision()])
    await tick()

    expect(target.querySelector('.result')).toBeNull()
  })

  /** The follow-up turn a rejected sweep earns carries the whole sweep, which is the point of
   *  coalescing: one turn, every group the user said no to. */
  it('renders every decision of a sweep answered at once', async () => {
    const target = render([
      decision({ verb: 'trash', what: '/Users/dana/Downloads/*.dmg' }),
      decision({ verb: 'move', what: '/Users/dana/Desktop/*.png', ops: 4 }),
    ])
    await tick()

    expect(target.querySelectorAll('li')).toHaveLength(2)
    expect(target.textContent).toContain('/Users/dana/Desktop/*.png')
  })

  /** A path is attacker-controlled (it comes off the user's disk), so it renders as escaped
   *  plain text and never as markup. */
  it('renders a path as text rather than as markup', async () => {
    const target = render([decision({ what: '<img src=x onerror=alert(1)>' })])
    await tick()

    expect(target.querySelector('img')).toBeNull()
    expect(target.textContent).toContain('<img src=x onerror=alert(1)>')
  })
})
