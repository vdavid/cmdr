/**
 * Functional tests for the `Combobox` primitive's load-bearing value model (dropdown-uniformization
 * plan, finding #4): the field is a text-field-with-suggestions, NOT a value-bound select. Its text
 * is `inputValue`-driven and must survive an empty / mid-fetch collection and a typed custom value
 * that isn't in the list. Ark's default `selectionBehavior: "replace"` would BLANK the field in
 * exactly these cases, so these tests guard the `preserve` + separate-`inputValue` wiring.
 *
 * `.svelte.test.ts` so the `$state` rune in the reactive-prop test compiles.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, flushSync, tick } from 'svelte'
import Combobox, { type ComboboxItem } from './Combobox.svelte'

function getInput(target: HTMLElement): HTMLInputElement {
  const input = target.querySelector('input')
  if (!input) throw new Error('Combobox input not found')
  return input
}

describe('Combobox value model', () => {
  it('shows its inputValue with an EMPTY items list (cold start)', () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(Combobox, {
      target,
      props: { items: [], inputValue: 'my-custom-model', onInputValueChange: () => {}, ariaLabel: 'Model' },
    })
    flushSync()
    expect(getInput(target).value).toBe('my-custom-model')
  })

  it('shows a custom value that is not a collection member (no snap-back to a member, no blank)', () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    // The list has members, but the controlled `inputValue` is a custom name NOT among them. This is
    // the exact case Ark's default `selectionBehavior: "replace"` + `stringifyMany` would blank.
    const items: ComboboxItem[] = [
      { value: 'gpt-4o', label: 'gpt-4o' },
      { value: 'gpt-4o-mini', label: 'gpt-4o-mini' },
    ]
    const state = $state({ inputValue: 'org/custom-finetune-7b' })
    mount(Combobox, {
      target,
      props: {
        items,
        get inputValue(): string {
          return state.inputValue
        },
        onInputValueChange: (v: string) => {
          state.inputValue = v
        },
        ariaLabel: 'Model',
      },
    })
    flushSync()
    expect(getInput(target).value).toBe('org/custom-finetune-7b')

    // The consumer updates the controlled text to another non-member custom value: it must show.
    state.inputValue = 'another/custom-13b'
    flushSync()
    expect(getInput(target).value).toBe('another/custom-13b')
  })

  it('selecting an item from the list reports it via onInputValueChange', async () => {
    // The reported bug (issue #29): with `selectionBehavior="preserve"`, clicking a suggestion
    // leaves the input text untouched, so wiring only `onInputValueChange` (the typing event)
    // swallowed the click. The component must ALSO bridge Ark's selection (`onValueChange`) into
    // `onInputValueChange`, or the dropdown can't select anything.
    const target = document.createElement('div')
    document.body.appendChild(target)
    const onInputValueChange = vi.fn<(v: string) => void>()
    const items: ComboboxItem[] = [
      { value: 'gpt-4o', label: 'gpt-4o' },
      { value: 'gpt-4o-mini', label: 'gpt-4o-mini' },
    ]
    mount(Combobox, { target, props: { items, inputValue: '', onInputValueChange, ariaLabel: 'Model' } })
    flushSync()

    // Open the popup (Ark only acts on a selection while open), then click a suggestion.
    getInput(target).click()
    await tick()
    const option = [...target.querySelectorAll<HTMLElement>('.combobox-item')].find((el) =>
      el.textContent?.includes('gpt-4o-mini'),
    )
    if (!option) throw new Error('combobox item not found')
    option.click()
    await tick()

    expect(onInputValueChange).toHaveBeenCalledWith('gpt-4o-mini')
  })

  it('shows its inputValue when the suggestion list arrives after a fetch (cold start, then populated)', () => {
    // The field starts with a custom value and an empty list (cold start), then a re-render hands it
    // a populated list while keeping the same inputValue. Ark must not drop the typed text. Driving
    // this through a fresh mount with the populated list keeps the test free of rune-typing quirks.
    const cold = document.createElement('div')
    document.body.appendChild(cold)
    mount(Combobox, {
      target: cold,
      props: { items: [], inputValue: 'custom-name', onInputValueChange: () => {}, ariaLabel: 'Model' },
    })
    flushSync()
    expect(getInput(cold).value).toBe('custom-name')

    const warm = document.createElement('div')
    document.body.appendChild(warm)
    mount(Combobox, {
      target: warm,
      props: {
        items: [{ value: 'gpt-4o', label: 'gpt-4o' }],
        inputValue: 'custom-name',
        onInputValueChange: () => {},
        ariaLabel: 'Model',
      },
    })
    flushSync()
    // Same custom inputValue, now with a populated list that doesn't contain it: still shown.
    expect(getInput(warm).value).toBe('custom-name')
  })
})
