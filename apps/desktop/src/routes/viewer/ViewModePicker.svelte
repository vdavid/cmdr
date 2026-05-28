<script lang="ts">
  /**
   * View mode picker. Ships with only "Text" today; the dropdown structure is
   * placeholder for future modes (hex, image, HTML strip). Disabled because
   * there's nothing to switch to yet.
   */
  type ViewMode = 'text'

  type Props = {
    value: ViewMode
    onChange?: (mode: ViewMode) => void
  }

  const { value, onChange }: Props = $props()

  function handleChange(event: Event) {
    const target = event.target as HTMLSelectElement
    onChange?.(target.value as ViewMode)
  }
</script>

<select
  class="view-mode-picker"
  aria-label="View mode"
  {value}
  disabled
  onchange={handleChange}
>
  <option value="text">Text</option>
</select>

<style>
  .view-mode-picker {
    appearance: auto;
    background: var(--color-bg-secondary);
    color: var(--color-text-primary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    padding: var(--spacing-xxs) var(--spacing-xs);
    font-size: var(--font-size-sm);
    cursor: not-allowed;
    opacity: 0.7;
  }

  .view-mode-picker:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 1px;
  }
</style>
