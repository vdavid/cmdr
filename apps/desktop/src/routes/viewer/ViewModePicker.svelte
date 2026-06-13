<script lang="ts">
  /**
   * View mode picker. Ships with only "Text" today; the dropdown structure is
   * placeholder for future modes (hex, image, HTML strip). Disabled because
   * there's nothing to switch to yet.
   */
  import Select, { type SelectItem } from '$lib/ui/Select.svelte'

  type ViewMode = 'text'

  type Props = {
    value: ViewMode
    onChange?: (mode: ViewMode) => void
  }

  const { value, onChange }: Props = $props()

  const items: SelectItem[] = [{ value: 'text', label: 'Text' }]

  function handleChange(picked: string) {
    onChange?.(picked as ViewMode)
  }
</script>

<Select {items} {value} disabled ariaLabel="View mode" onChange={handleChange} />
