<script lang="ts">
  import Select, { type SelectItem } from '#lib/ui/Select.svelte'
  import type { ViewerContentKind } from '#lib/ipc/bindings.js'
  import { availableMediaKind, mediaKindLabel, viewAsMediaLabel } from './media-view'
  import type { ViewerDisplayMode } from './viewer-view-mode'
  import { tString } from '#lib/intl/messages.svelte.js'
  import { shortcutAsText } from '#lib/shortcuts/reactive-shortcuts.svelte.js'
  import type { CommandId } from '#lib/commands/index.js'

  type Props = {
    mode: ViewerDisplayMode
    kind: ViewerContentKind
    lastMediaKind: ViewerContentKind | null
    onModeChange: (mode: ViewerDisplayMode) => void
  }

  const { mode, kind, lastMediaKind, onModeChange }: Props = $props()
  const mediaKind = $derived(availableMediaKind(kind, lastMediaKind))

  /** An option's label with its key appended, `Text (1)`; the key is the user's (remappable), and none shows no suffix. */
  function withKey(label: string, command: CommandId): string {
    const key = shortcutAsText(command, 'menuItem')
    return key === undefined ? label : `${label} (${key})`
  }

  const items = $derived<SelectItem[]>([
    ...(mediaKind
      ? [
          {
            value: 'media',
            label: withKey(mode === 'media' ? mediaKindLabel(mediaKind) : viewAsMediaLabel(mediaKind), 'viewer.viewAsMedia'),
          },
        ]
      : []),
    { value: 'text', label: withKey(tString('viewer.toolbar.viewMode.text'), 'viewer.viewAsText') },
    { value: 'binary', label: withKey(tString('viewer.toolbar.viewMode.binary'), 'viewer.viewAsBinary') },
    { value: 'hex', label: withKey(tString('viewer.toolbar.viewMode.hex'), 'viewer.viewAsHex') },
  ])
</script>

<Select {items} value={mode} ariaLabel={tString('viewer.toolbar.viewMode.ariaLabel')} onChange={(picked) => { onModeChange(picked as ViewerDisplayMode); }} />
