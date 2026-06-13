<script lang="ts">
  import Select, { type SelectItem } from '$lib/ui/Select.svelte'
  import type { EncodingChoice, EncodingGroup, FileEncoding } from '$lib/ipc/bindings'

  type Props = {
    /** Currently active encoding (drives the selected value). */
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

  /** Heading text for each backend group discriminator. */
  const groupToHeadingMap: Record<EncodingGroup, string> = {
    unicode: 'Unicode',
    western: 'Western',
  }

  /**
   * Map each backend `EncodingChoice` to a `SelectItem`, keeping the backend's
   * order. The `group` heading buckets the items into Unicode/Western sections;
   * the detected encoding keeps its "(Detected)" suffix as label text.
   */
  const items = $derived<SelectItem[]>(
    options.map((choice) => ({
      value: choice.encoding,
      label: choice.encoding === detected ? `${choice.label} (Detected)` : choice.label,
      group: groupToHeadingMap[choice.group],
    })),
  )

  function handleChange(picked: string) {
    onChange(picked as FileEncoding)
  }
</script>

<Select {items} {value} {disabled} ariaLabel="Encoding" onChange={handleChange} />
