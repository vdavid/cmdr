<script lang="ts">
    import type { Snippet } from 'svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { isSettingManaged } from '$lib/managed-policy/managed-policy.svelte'
    import { provideSectionRows } from './section-rows.svelte'

    interface Props {
        title: string
        /** Optional trailing content beside the title (e.g. a feature-status badge). */
        badge?: Snippet
        children: Snippet
    }

    const { title, badge, children }: Props = $props()

    // The rows register themselves, so this line appears wherever a managed row does, with no
    // per-section list. Plain text in reading order: a keyboard or VoiceOver user meets the reason
    // before the disabled controls, which native `disabled` takes out of the Tab order.
    const rowIds = provideSectionRows()
    const anyManaged = $derived([...rowIds].some((id) => isSettingManaged(id)))
</script>

<div class="section">
    <div class="section-header">
        <h2 class="section-title">{title}</h2>
        {#if badge}{@render badge()}{/if}
    </div>
    {#if anyManaged}
        <p class="managed-section-note">
            <span class="managed-note-icon"><Icon name="info" size={14} aria-hidden="true" /></span>
            <span>{tString('settings.managed.sectionNote')}</span>
        </p>
    {/if}
    {@render children()}
</div>

<style>
    .section {
        margin-bottom: var(--spacing-lg);
    }

    .section-header {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        /* The larger bottom margin gives the title breathing room from the rows
           below without a horizontal hairline, matching System Settings. */
        margin: 0 0 var(--spacing-lg);
    }

    .section-title {
        font-size: var(--font-size-lg);
        font-weight: 600;
        color: var(--color-text-primary);
        margin: 0;
    }

    .managed-section-note {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-xs);
        margin: 0 0 var(--spacing-md);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    /* Centers the glyph on the first text line, however the note wraps. */
    .managed-note-icon {
        display: flex;
        align-items: center;
        height: 1lh;
    }
</style>
