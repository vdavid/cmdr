<script lang="ts">
  import { SvelteMap } from 'svelte/reactivity'

  import type { EncodingChoice, EncodingGroup, FileEncoding } from '$lib/ipc/bindings'

  type Props = {
    /** Currently active encoding (drives the `<select>` value). */
    value: FileEncoding
    /** Encoding that auto-detection picked at session open. Gets a "(Detected)" suffix. */
    detected: FileEncoding
    /** All selectable encodings, ordered for the dropdown. Backend-authoritative. */
    options: EncodingChoice[]
    /** Whether the picker is currently disabled (for example during a rebuild). */
    disabled?: boolean
    /** Called when the user picks a different encoding. */
    onChange: (encoding: FileEncoding) => void
  }

  const { value, detected, options, disabled = false, onChange }: Props = $props()

  /**
   * Split the options list into a Unicode group and a Western group, preserving
   * the backend's order within each group. The labels for the `<optgroup>`s match
   * the backend's `EncodingGroup` discriminator.
   */
  const grouped = $derived.by(() => {
    const groups = new SvelteMap<EncodingGroup, EncodingChoice[]>()
    for (const choice of options) {
      const list = groups.get(choice.group) ?? []
      list.push(choice)
      groups.set(choice.group, list)
    }
    return groups
  })

  function decorate(choice: EncodingChoice): string {
    return choice.encoding === detected ? `${choice.label} (Detected)` : choice.label
  }

  function handleChange(event: Event) {
    const target = event.target as HTMLSelectElement
    onChange(target.value as FileEncoding)
  }

  const unicodeOptions = $derived(grouped.get('unicode') ?? [])
  const westernOptions = $derived(grouped.get('western') ?? [])
</script>

<select
  class="encoding-picker"
  aria-label="Encoding"
  {value}
  {disabled}
  onchange={handleChange}
>
  {#if unicodeOptions.length > 0}
    <optgroup label="Unicode">
      {#each unicodeOptions as choice (choice.encoding)}
        <option value={choice.encoding}>{decorate(choice)}</option>
      {/each}
    </optgroup>
  {/if}
  {#if westernOptions.length > 0}
    <optgroup label="Western">
      {#each westernOptions as choice (choice.encoding)}
        <option value={choice.encoding}>{decorate(choice)}</option>
      {/each}
    </optgroup>
  {/if}
</select>

<style>
  .encoding-picker {
    appearance: auto;
    background: var(--color-bg-secondary);
    color: var(--color-text-primary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    padding: var(--spacing-xxs) var(--spacing-xs);
    font-size: var(--font-size-sm);
  }

  .encoding-picker:disabled {
    opacity: 0.6;
  }

  .encoding-picker:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 1px;
  }
</style>
