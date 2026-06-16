<script lang="ts">
  /**
   * View-mode picker. Shows the detected content kind (Image / PDF / Text) and
   * offers the two-way switch between rendered media and the raw text view:
   *
   * - Media kind (image / PDF): the current kind plus "View as text", which re-opens
   *   the file as a fresh full text session (the viewer swaps to it).
   * - Text view of a media file: "Text" plus "View as image" / "View as PDF", which
   *   re-opens the file naturally so it re-renders as media. `lastMediaKind` carries
   *   the file's natural kind so the text view knows what to switch back to.
   * - Genuine text file (no `lastMediaKind`): a single disabled "Text" option, since
   *   there's nothing to switch to.
   */
  import Select, { type SelectItem } from '$lib/ui/Select.svelte'
  import type { ViewerContentKind } from '$lib/ipc/bindings'
  import { mediaKindLabel, isMediaKind, viewAsMediaLabel } from './media-view'
  import { tString } from '$lib/intl/messages.svelte'

  // The switch items use sentinel values distinct from the three content kinds.
  const VIEW_AS_TEXT = 'viewAsText'
  const VIEW_AS_MEDIA = 'viewAsMedia'

  type Props = {
    kind: ViewerContentKind
    /**
     * The file's natural media kind, remembered across a switch to text. Non-null
     * while reading a media file as text (enables the reverse "View as image / PDF"
     * switch); null for a genuine text file.
     */
    lastMediaKind?: ViewerContentKind | null
    /** Called when the user picks "View as text" on a media file. */
    onViewAsText?: () => void
    /** Called when the user picks "View as image / PDF" from the text view of a media file. */
    onViewAsMedia?: () => void
  }

  const { kind, lastMediaKind = null, onViewAsText, onViewAsMedia }: Props = $props()

  // True while reading a media file as text: the picker offers the reverse switch.
  const isTextOfMedia = $derived(kind === 'text' && lastMediaKind !== null)

  const items = $derived<SelectItem[]>(
    isMediaKind(kind)
      ? [
          { value: kind, label: mediaKindLabel(kind) },
          { value: VIEW_AS_TEXT, label: tString('viewer.toolbar.viewMode.viewAsText') },
        ]
      : isTextOfMedia && lastMediaKind !== null
        ? [
            { value: 'text', label: tString('viewer.toolbar.viewMode.text') },
            { value: VIEW_AS_MEDIA, label: viewAsMediaLabel(lastMediaKind) },
          ]
        : [{ value: 'text', label: tString('viewer.toolbar.viewMode.text') }],
  )

  // A genuine text file has only one option, so the picker is inert (matches the
  // encoding picker's disabled-when-nothing-to-do convention). A media file and
  // the text-of-a-media-file case both have a real switch, so they stay enabled.
  const disabled = $derived(!isMediaKind(kind) && !isTextOfMedia)

  function handleChange(picked: string): void {
    if (picked === VIEW_AS_TEXT) onViewAsText?.()
    else if (picked === VIEW_AS_MEDIA) onViewAsMedia?.()
  }
</script>

<Select {items} value={kind} {disabled} ariaLabel={tString('viewer.toolbar.viewMode.ariaLabel')} onChange={handleChange} />
