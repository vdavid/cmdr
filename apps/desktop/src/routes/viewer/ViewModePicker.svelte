<script lang="ts">
  import Select, { type SelectItem } from '$lib/ui/Select.svelte'
  import type { ViewerContentKind } from '$lib/ipc/bindings'
  import { availableMediaKind, mediaKindLabel, viewAsMediaLabel } from './media-view'
  import type { ViewerDisplayMode } from './viewer-view-mode'
  import { tString } from '$lib/intl/messages.svelte'

  type Props = {
    mode: ViewerDisplayMode
    kind: ViewerContentKind
    lastMediaKind: ViewerContentKind | null
    onModeChange: (mode: ViewerDisplayMode) => void
  }

  const { mode, kind, lastMediaKind, onModeChange }: Props = $props()
  const mediaKind = $derived(availableMediaKind(kind, lastMediaKind))
  const items = $derived<SelectItem[]>([
    ...(mediaKind
      ? [{ value: 'media', label: `${mode === 'media' ? mediaKindLabel(mediaKind) : viewAsMediaLabel(mediaKind)} (0)` }]
      : []),
    { value: 'text', label: `${tString('viewer.toolbar.viewMode.text')} (1)` },
    { value: 'binary', label: `${tString('viewer.toolbar.viewMode.binary')} (2)` },
    { value: 'hex', label: `${tString('viewer.toolbar.viewMode.hex')} (3)` },
  ])
</script>

<Select {items} value={mode} ariaLabel={tString('viewer.toolbar.viewMode.ariaLabel')} onChange={(picked) => { onModeChange(picked as ViewerDisplayMode); }} />
