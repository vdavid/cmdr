<!--
  One attachment reference chip: a file/folder glyph plus the escaped basename. In the
  composer it carries a remove button; under a sent user message it's read-only.

  The name is filesystem-derived (attacker-controllable on a network share), so it renders
  as plain {text} (Svelte auto-escapes), NEVER {@html}.
-->
<script lang="ts">
    import Icon from '$lib/ui/Icon.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { attachmentBasename } from './ask-cmdr-attachments'
    import type { AttachmentRef } from '$lib/tauri-commands'

    interface Props {
        attachment: AttachmentRef
        onRemove?: (path: string) => void
    }
    const { attachment, onRemove }: Props = $props()

    const name = $derived(attachmentBasename(attachment.path))
</script>

<span class="chip" title={attachment.path}>
    <Icon name={attachment.kind === 'folder' ? 'folder' : 'file'} size={12} aria-hidden="true" />
    <span class="chip-name">{name}</span>
    {#if onRemove}
        <button type="button" class="chip-remove" aria-label={tString('askCmdr.attachment.remove')} onclick={() => { onRemove(attachment.path); }}>
            <Icon name="x" size={12} aria-hidden="true" />
        </button>
    {/if}
</span>

<style>
    .chip {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xxs);
        max-width: 100%;
        padding: var(--spacing-xxs) var(--spacing-xs);
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        background: var(--color-bg-tertiary);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
    }

    .chip-name {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .chip-remove {
        display: flex;
        align-items: center;
        justify-content: center;
        padding: 0;
        border: none;
        background: none;
        color: var(--color-text-secondary);
        border-radius: var(--radius-xs);
    }

    .chip-remove:hover {
        color: var(--color-text-primary);
        background: var(--color-bg-secondary);
    }
</style>
