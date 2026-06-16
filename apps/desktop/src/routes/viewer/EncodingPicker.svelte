<script lang="ts">
  import Select, { type SelectItem } from '$lib/ui/Select.svelte'
  import type { EncodingChoice, EncodingGroup, FileEncoding } from '$lib/ipc/bindings'
  import { tString } from '$lib/intl/messages.svelte'

  type Props = {
    /** Currently active encoding (drives the selected value). Empty when there's no text
     *  session yet (media mode), so the picker shows its `placeholder`, disabled. */
    value: FileEncoding | ''
    /** Encoding that auto-detection picked at session open. Gets a "(Detected)" marker. */
    detected: FileEncoding
    /** All selectable encodings, ordered for the dropdown. Backend-authoritative. */
    options: EncodingChoice[]
    /** Whether the picker is currently disabled (for example during a rebuild, or in media mode). */
    disabled?: boolean
    /** Shown when nothing is selected (media mode, before any text session exists). */
    placeholder?: string
    /** Called when the user picks a different encoding. */
    onChange: (encoding: FileEncoding) => void
  }

  const {
    value,
    detected,
    options,
    disabled = false,
    placeholder = tString('viewer.toolbar.encoding.placeholder'),
    onChange,
  }: Props = $props()

  /** Heading text for each backend group discriminator. */
  const groupToHeadingMap: Record<EncodingGroup, string> = {
    unicode: tString('viewer.toolbar.encoding.group.unicode'),
    western: tString('viewer.toolbar.encoding.group.western'),
  }

  /**
   * Map each backend `EncodingChoice` to a `SelectItem`, keeping the backend's
   * order. The `group` heading buckets the items into Unicode/Western sections;
   * the detected encoding keeps its "(Detected)" suffix as label text.
   */
  const items = $derived<SelectItem[]>(
    options.map((choice) => ({
      value: choice.encoding,
      label:
        choice.encoding === detected
          ? tString('viewer.toolbar.encoding.detectedSuffix', { label: choice.label })
          : choice.label,
      group: groupToHeadingMap[choice.group],
    })),
  )

  function handleChange(picked: string) {
    onChange(picked as FileEncoding)
  }
</script>

<Select
  {items}
  {value}
  {disabled}
  {placeholder}
  ariaLabel={tString('viewer.toolbar.encoding.placeholder')}
  onChange={handleChange}
/>
