<!--
  The Ask Cmdr chat rail: a toggleable right-side panel next to the panes. Header (title +
  ALPHA badge + new-chat + close), the scrolling thread, the getting-long nudge, and the
  composer. A left-edge drag handle resizes it (persisted). Below ~900px it overlays the
  right pane instead of compressing the panes below their min-width.
-->
<script lang="ts">
    import { tick } from 'svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import StatusBadge from '$lib/ui/StatusBadge.svelte'
    import { getBadgeStatus } from '$lib/feature-status'
    import { tString } from '$lib/intl/messages.svelte'
    import AskCmdrComposer from './AskCmdrComposer.svelte'
    import AskCmdrMessage from './AskCmdrMessage.svelte'
    import { askCmdrState, closeRail, isOverSoftCap, newChat, setRailWidth } from './ask-cmdr-trigger.svelte'

    const badgeStatus = getBadgeStatus('ask-cmdr')

    let listElement = $state<HTMLDivElement | null>(null)

    // Keep the newest message in view as the thread grows or text streams in.
    const lastText = $derived(askCmdrState.messages.at(-1))
    $effect(() => {
        // Track the message count and the live tail so streaming deltas also scroll.
        void askCmdrState.messages.length
        void (lastText && lastText.kind === 'assistant' ? lastText.text : '')
        void tick().then(() => {
            if (listElement) listElement.scrollTop = listElement.scrollHeight
        })
    })

    // Left-edge resize: dragging left widens (the rail hugs the right edge).
    let dragStartX = 0
    let dragStartWidth = 0
    function onHandlePointerDown(event: PointerEvent): void {
        event.preventDefault()
        dragStartX = event.clientX
        dragStartWidth = askCmdrState.width
        ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
    }
    function onHandlePointerMove(event: PointerEvent): void {
        if (!(event.currentTarget as HTMLElement).hasPointerCapture(event.pointerId)) return
        setRailWidth(dragStartWidth + (dragStartX - event.clientX))
    }
    function onHandlePointerUp(event: PointerEvent): void {
        ;(event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId)
    }
</script>

<aside class="ask-cmdr-rail" style="width: {askCmdrState.width}px" aria-label={tString('askCmdr.title')}>
    <div
        class="resize-handle"
        role="separator"
        aria-orientation="vertical"
        aria-label={tString('askCmdr.title')}
        onpointerdown={onHandlePointerDown}
        onpointermove={onHandlePointerMove}
        onpointerup={onHandlePointerUp}
        ondblclick={() => { setRailWidth(340); }}
    ></div>

    <header class="rail-header">
        <span class="header-glyph"><Icon name="sparkles" size={15} aria-hidden="true" /></span>
        <span class="header-title">{tString('askCmdr.title')}</span>
        {#if badgeStatus}
            <StatusBadge status={badgeStatus} />
        {/if}
        <span class="header-actions">
            <button type="button" class="icon-button" onclick={newChat} aria-label={tString('askCmdr.newChat')}>
                <Icon name="file-plus" size={16} aria-hidden="true" />
            </button>
            <button type="button" class="icon-button" onclick={closeRail} aria-label={tString('askCmdr.close')}>
                <Icon name="x" size={16} aria-hidden="true" />
            </button>
        </span>
    </header>

    <div class="rail-body" bind:this={listElement}>
        {#if askCmdrState.messages.length === 0}
            <div class="empty">
                <span class="empty-glyph"><Icon name="sparkles" size={28} aria-hidden="true" /></span>
                <p class="empty-title">{tString('askCmdr.empty.title')}</p>
                <p class="empty-hint">{tString('askCmdr.empty.hint')}</p>
            </div>
        {:else}
            {#each askCmdrState.messages as message, index (index)}
                <AskCmdrMessage {message} />
            {/each}
            {#if isOverSoftCap()}
                <div class="soft-cap">
                    <span>{tString('askCmdr.softCap.message')}</span>
                    <button type="button" class="soft-cap-action" onclick={newChat}>
                        {tString('askCmdr.softCap.action')}
                    </button>
                </div>
            {/if}
        {/if}
    </div>

    <AskCmdrComposer />
</aside>

<style>
    .ask-cmdr-rail {
        position: relative;
        display: flex;
        flex-direction: column;
        flex: none;
        height: 100%;
        min-height: 0;
        background: var(--color-bg-secondary);
        border-left: 1px solid var(--color-border);
    }

    .resize-handle {
        position: absolute;
        top: 0;
        left: -3px;
        width: 6px;
        height: 100%;
        cursor: col-resize;
        z-index: var(--z-sticky);
    }

    .rail-header {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-sm);
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .header-glyph {
        display: flex;
        color: var(--color-accent-text);
    }

    .header-title {
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .header-actions {
        display: flex;
        gap: var(--spacing-xxs);
        margin-left: auto;
    }

    .icon-button {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 28px;
        height: 28px;
        border: none;
        background: none;
        color: var(--color-text-secondary);
        border-radius: var(--radius-sm);
    }

    .icon-button:hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    .rail-body {
        flex: 1;
        min-height: 0;
        overflow-y: auto;
        padding: var(--spacing-md);
    }

    .empty {
        display: flex;
        flex-direction: column;
        align-items: center;
        text-align: center;
        gap: var(--spacing-xs);
        margin-top: var(--spacing-lg);
        color: var(--color-text-secondary);
    }

    .empty-glyph {
        color: var(--color-accent-text);
    }

    .empty-title {
        margin: 0;
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .empty-hint {
        margin: 0;
        font-size: var(--font-size-sm);
        line-height: 1.5;
    }

    .soft-cap {
        display: flex;
        flex-direction: column;
        align-items: flex-start;
        gap: var(--spacing-xs);
        padding: var(--spacing-sm);
        margin-top: var(--spacing-xs);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-md);
    }

    .soft-cap-action {
        padding: var(--spacing-xxs) var(--spacing-sm);
        font: inherit;
        font-size: var(--font-size-sm);
        color: var(--color-accent-text);
        background: none;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
    }

    .soft-cap-action:hover {
        background: var(--color-bg-secondary);
    }

    /* Narrow windows: overlay the right pane rather than compress the panes past their min. */
    @media (width <= 900px) {
        .ask-cmdr-rail {
            position: absolute;
            top: 0;
            right: 0;
            bottom: 0;
            z-index: var(--z-overlay);
            box-shadow: var(--shadow-lg);
        }
    }
</style>
