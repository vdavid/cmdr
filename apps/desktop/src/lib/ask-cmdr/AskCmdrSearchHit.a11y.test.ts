/**
 * Tier 3 a11y tests for `AskCmdrSearchHit.svelte`: one search result's content (title, age,
 * marked excerpt, archived badge, the agent glyph), mounted inside the `option` its panel gives it.
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import type { ConversationSearchHit } from '#lib/tauri-commands/index.js'
import AskCmdrSearchHit from './AskCmdrSearchHit.svelte'

const NOW = 1_800_000_000

function hit(overrides: Partial<ConversationSearchHit> = {}): ConversationSearchHit {
  return {
    conversationId: 1,
    title: 'Budget review',
    updatedAt: NOW - 2 * 86_400,
    archived: false,
    snippet: [
      { text: 'the quarterly ', matched: false },
      { text: 'budget', matched: true },
      { text: ' spreadsheet', matched: false },
    ],
    origin: null,
    wakePlace: null,
    ...overrides,
  }
}

async function mountInOption(props: ConversationSearchHit): Promise<HTMLElement> {
  const listbox = document.createElement('ul')
  listbox.setAttribute('role', 'listbox')
  listbox.setAttribute('aria-label', 'Matching chats')
  const option = document.createElement('li')
  option.setAttribute('role', 'option')
  option.setAttribute('aria-selected', 'true')
  listbox.appendChild(option)
  document.body.appendChild(listbox)
  mount(AskCmdrSearchHit, { target: option, props: { hit: props, nowSecs: NOW } })
  await tick()
  return listbox
}

describe('AskCmdrSearchHit a11y', () => {
  it('a plain hit with a marked excerpt has no a11y violations', async () => {
    await expectNoA11yViolations(await mountInOption(hit()))
  })

  it('a wake hit, archived, with no excerpt has no a11y violations', async () => {
    await expectNoA11yViolations(
      await mountInOption(
        hit({
          origin: 'notification',
          wakePlace: { folder: '~/tmp', otherFolders: 1 },
          snippet: [],
          archived: true,
        }),
      ),
    )
  })
})
