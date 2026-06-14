<script lang="ts">
  /**
   * View-mode picker. Shows the detected content kind (Image / PDF / Text). For a
   * media kind (image or PDF) it also offers "View as text", which re-opens the file
   * as a fresh full text session (the viewer swaps to it). For a text file there's
   * nothing to switch to, so the picker is disabled.
   */
  import Select, { type SelectItem } from '$lib/ui/Select.svelte'
  import type { ViewerContentKind } from '$lib/ipc/bindings'
  import { mediaKindLabel, isMediaKind } from './media-view'

  // The "View as text" item uses a sentinel value distinct from the three kinds.
  const VIEW_AS_TEXT = 'viewAsText'

  type Props = {
    kind: ViewerContentKind
    /** Called when the user picks "View as text" on a media file. */
    onViewAsText?: () => void
  }

  const { kind, onViewAsText }: Props = $props()

  const items = $derived<SelectItem[]>(
    isMediaKind(kind)
      ? [
          { value: kind, label: mediaKindLabel(kind) },
          { value: VIEW_AS_TEXT, label: 'View as text' },
        ]
      : [{ value: 'text', label: 'Text' }],
  )

  // A text file has only one option, so the picker is inert (matches the
  // encoding picker's disabled-when-nothing-to-do convention).
  const disabled = $derived(!isMediaKind(kind))

  function handleChange(picked: string): void {
    if (picked === VIEW_AS_TEXT) onViewAsText?.()
  }
</script>

<Select {items} value={kind} {disabled} ariaLabel="View mode" onChange={handleChange} />
