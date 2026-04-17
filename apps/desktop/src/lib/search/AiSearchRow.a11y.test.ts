/**
 * Tier 3 a11y tests for `AiSearchRow.svelte`.
 *
 * AI prompt input + Ask AI button. Shown when AI search is enabled.
 * Tests cover the empty/idle state, a populated prompt, a disabled
 * state, and the error/status pairs.
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import AiSearchRow from './AiSearchRow.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('AiSearchRow a11y', () => {
  it('empty prompt (idle) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AiSearchRow, {
      target,
      props: {
        inputElement: undefined,
        aiPrompt: '',
        onPromptInput: () => {},
        onAiSearch: () => {},
        disabled: false,
        caveatText: '',
        aiStatus: '',
        aiError: '',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('populated prompt with caveat has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AiSearchRow, {
      target,
      props: {
        inputElement: undefined,
        aiPrompt: 'photos from last weekend',
        onPromptInput: () => {},
        onAiSearch: () => {},
        disabled: false,
        caveatText: 'AI-translated query. Review before trusting results.',
        aiStatus: '',
        aiError: '',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AiSearchRow, {
      target,
      props: {
        inputElement: undefined,
        aiPrompt: 'large videos',
        onPromptInput: () => {},
        onAiSearch: () => {},
        disabled: true,
        caveatText: '',
        aiStatus: '',
        aiError: '',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with AI status message has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AiSearchRow, {
      target,
      props: {
        inputElement: undefined,
        aiPrompt: 'pdfs modified this week',
        onPromptInput: () => {},
        onAiSearch: () => {},
        disabled: false,
        caveatText: '',
        aiStatus: 'Translating your query...',
        aiError: '',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with AI error has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AiSearchRow, {
      target,
      props: {
        inputElement: undefined,
        aiPrompt: 'huge files',
        onPromptInput: () => {},
        onAiSearch: () => {},
        disabled: false,
        caveatText: '',
        aiStatus: '',
        aiError: "The AI couldn't translate this query. Try again or use the manual fields.",
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
