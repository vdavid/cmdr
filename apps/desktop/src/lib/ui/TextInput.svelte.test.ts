/**
 * Behavior tests for `TextInput` / `TextArea`.
 *
 * The load-bearing invariants are (1) the value contract — the field reports edits and follows a
 * value the caller pushes back, which is what makes masking and external resets work — and (2) the
 * chrome contract every call site relies on (radius / variant / invalid classes, native attribute
 * passthrough, the leading-icon slot).
 *
 * `bind:inputElement` isn't asserted here: `mount()` can't observe a bindable writing back into a
 * plain props object. `svelte-check` type-checks the binding at every call site, and the
 * focus-on-mount dialogs cover it end to end in the Playwright suite.
 */

import { describe, expect, it } from 'vitest'
import { mount, tick } from 'svelte'
import TextInput from './TextInput.svelte'
import TextArea from './TextArea.svelte'

function mountTarget(): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  return target
}

function typeInto(input: HTMLInputElement | HTMLTextAreaElement, text: string) {
  input.value = text
  input.dispatchEvent(new Event('input', { bubbles: true }))
}

describe('TextInput', () => {
  it('renders the value and reports edits through oninput', () => {
    const target = mountTarget()
    const seen: string[] = []
    mount(TextInput, {
      target,
      props: {
        value: 'photos',
        ariaLabel: 'Folder name',
        oninput: (event: Event) => seen.push((event.currentTarget as HTMLInputElement).value),
      },
    })
    const input = target.querySelector('input')
    expect(input?.value).toBe('photos')

    typeInto(input as HTMLInputElement, 'photos-2026')
    expect(seen).toEqual(['photos-2026'])
  })

  it('follows a value the caller pushes back (masking, external reset)', async () => {
    const target = mountTarget()
    // `SettingPasswordInput`'s shape: the caller re-renders a masked / reset value
    // and the field must show it, not whatever was last typed.
    const props = $state({ value: 'sk-live-1234', ariaLabel: 'API key' })
    mount(TextInput, { target, props })
    const input = target.querySelector('input') as HTMLInputElement
    expect(input.value).toBe('sk-live-1234')

    typeInto(input, 'sk-live-5678')
    props.value = '••••••••5678'
    await tick()
    expect(input.value).toBe('••••••••5678')
  })

  it('leaves normal typing alone', async () => {
    const target = mountTarget()
    const props = $state({ value: '', ariaLabel: 'Folder name' })
    mount(TextInput, { target, props })
    const input = target.querySelector('input') as HTMLInputElement

    typeInto(input, 'new folder')
    await tick()
    expect(input.value).toBe('new folder')
  })

  it('defaults to the lg radius and switches to the pill shape on demand', () => {
    const plain = mountTarget()
    mount(TextInput, { target: plain, props: { value: '', ariaLabel: 'Plain' } })
    expect(plain.querySelector('.text-field')?.classList.contains('text-field-radius-lg')).toBe(true)

    const pill = mountTarget()
    mount(TextInput, { target: pill, props: { value: '', ariaLabel: 'Pill', radius: 'full' } })
    expect(pill.querySelector('.text-field')?.classList.contains('text-field-radius-full')).toBe(true)
  })

  it('marks the invalid state on both the frame and the control', () => {
    const target = mountTarget()
    mount(TextInput, { target, props: { value: 'bad', ariaLabel: 'Address', invalid: true } })
    expect(target.querySelector('.text-field')?.classList.contains('text-field-invalid')).toBe(true)
    expect(target.querySelector('input')?.getAttribute('aria-invalid')).toBe('true')
  })

  it('drops the frame in the chromeless variant', () => {
    const target = mountTarget()
    mount(TextInput, { target, props: { value: '', ariaLabel: 'Inline', variant: 'chromeless' } })
    expect(target.querySelector('.text-field')?.classList.contains('text-field-chromeless')).toBe(true)
  })

  it('passes native attributes straight through', () => {
    const target = mountTarget()
    mount(TextInput, {
      target,
      props: {
        value: '',
        id: 'host',
        type: 'password',
        placeholder: 'Password',
        readonly: true,
        maxlength: 40,
        autocomplete: 'off',
        spellcheck: false,
      },
    })
    const input = target.querySelector('input') as HTMLInputElement
    expect(input.id).toBe('host')
    expect(input.type).toBe('password')
    expect(input.placeholder).toBe('Password')
    expect(input.readOnly).toBe(true)
    expect(input.maxLength).toBe(40)
    expect(input.getAttribute('autocomplete')).toBe('off')
    expect(input.getAttribute('spellcheck')).toBe('false')
  })

  it('renders a leading icon only when asked for one', () => {
    const bare = mountTarget()
    mount(TextInput, { target: bare, props: { value: '', ariaLabel: 'Bare' } })
    expect(bare.querySelectorAll('.text-field-affix')).toHaveLength(0)

    const withIcon = mountTarget()
    mount(TextInput, { target: withIcon, props: { value: '', ariaLabel: 'Search', leadingIcon: 'search' } })
    expect(withIcon.querySelectorAll('.text-field-affix')).toHaveLength(1)
    expect(withIcon.querySelector('.text-field-affix svg')).not.toBeNull()
  })
})

describe('TextArea', () => {
  it('renders the value and reports edits through oninput', () => {
    const target = mountTarget()
    const seen: string[] = []
    mount(TextArea, {
      target,
      props: {
        value: 'first line',
        ariaLabel: 'Feedback',
        rows: 4,
        oninput: (event: Event) => seen.push((event.currentTarget as HTMLTextAreaElement).value),
      },
    })
    const textarea = target.querySelector('textarea') as HTMLTextAreaElement
    expect(textarea.value).toBe('first line')
    expect(textarea.getAttribute('rows')).toBe('4')

    typeInto(textarea, 'second line')
    expect(seen).toEqual(['second line'])
  })

  it('shares the frame contract with TextInput', () => {
    const target = mountTarget()
    mount(TextArea, { target, props: { value: '', ariaLabel: 'Notes', resizable: false } })
    const frame = target.querySelector('.text-field')
    expect(frame?.classList.contains('text-field-multiline')).toBe(true)
    expect(frame?.classList.contains('text-field-radius-lg')).toBe(true)
    expect(target.querySelector('textarea')?.classList.contains('text-field-control-fixed')).toBe(true)
  })

  it('follows a value the caller pushes back', async () => {
    const target = mountTarget()
    const props = $state({ value: 'draft', ariaLabel: 'Feedback' })
    mount(TextArea, { target, props })
    const textarea = target.querySelector('textarea') as HTMLTextAreaElement

    typeInto(textarea, 'edited')
    props.value = ''
    await tick()
    expect(textarea.value).toBe('')
  })
})
