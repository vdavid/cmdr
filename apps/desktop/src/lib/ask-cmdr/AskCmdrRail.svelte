<!--
  The Ask Cmdr chat rail: a toggleable right-side panel next to the panes. Header (title +
  ALPHA badge + new-chat + close), the no-provider notice when one applies, the scrolling
  thread, the getting-long nudge, the composer, and the cost footer. A left-edge drag handle resizes it (persisted). Below ~900px it overlays the
  right pane instead of compressing the panes below their min-width.
-->
<script lang="ts">
    import { onDestroy, tick, untrack } from 'svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import CloseButton from '#lib/ui/CloseButton.svelte'
    import StatusBadge from '#lib/ui/StatusBadge.svelte'
    import { getBadgeStatus } from '#lib/feature-status.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { dependOn } from '#lib/utils/reactivity.js'
    import AskCmdrComposer from './AskCmdrComposer.svelte'
    import AskCmdrMessage from './AskCmdrMessage.svelte'
    import AskCmdrSessions from './AskCmdrSessions.svelte'
    import AskCmdrGate from './AskCmdrGate.svelte'
    import AskCmdrCostFooter from './AskCmdrCostFooter.svelte'
    import {
        askCmdrState,
        closeRail,
        ensureThreadLoaded,
        hasOlderMessages,
        dismissLongChatNudge,
        keepGoingPastSpend,
        longChatNudge,
        loadOlderMessages,
        markRailFocused,
        newChatShortcutText,
        openNewChat,
        setRailWidth,
        stopStreaming,
    } from './ask-cmdr-trigger.svelte'
    import AskCmdrSpendPause from './AskCmdrSpendPause.svelte'
    import { railGate } from './ask-cmdr-gate.svelte'
    import AskCmdrProviderNotice from './AskCmdrProviderNotice.svelte'
    import { providerGap, refreshSlotStatus, slotStatusState, watchSlotStatus } from './ask-cmdr-slot-status.svelte'
    import { contentSignature } from './ask-cmdr-rounds'
    import { cloudConsentState } from '#lib/ai/cloud-consent.svelte.js'
    import { getSetting, onSpecificSettingChange, type AiProvider } from '#lib/settings/index.js'
    import { dismissSessions, openSessions, sessionsState } from './ask-cmdr-sessions.svelte'
    import type { SelectionAttachmentsRead } from '#lib/file-explorer/pane/selection-attachments-read.js'
    import type { RevealPath } from './ask-cmdr-tool-line'

    interface Props {
        /** The composer's "Ask about selection" read, handed down from the explorer. */
        readSelection: () => Promise<SelectionAttachmentsRead>
        /** Shows a tool line's path in a pane, handed down from the explorer. */
        revealPath: RevealPath
    }

    const { readSelection, revealPath }: Props = $props()

    const badgeStatus = getBadgeStatus('ask-cmdr')

    // Which of the two gates (Ask Cmdr off, cloud AI off) stands in for the chat, if any.
    // `loading` (Cloud, consent not read yet) renders neither, so nothing flashes.
    let enabled = $state(getSetting('askCmdr.enabled'))
    $effect(() => onSpecificSettingChange('askCmdr.enabled', (v) => { enabled = v }))
    let provider = $state<AiProvider>(getSetting('ai.provider'))
    $effect(() => onSpecificSettingChange('ai.provider', (v) => { provider = v }))
    const gate = $derived(railGate({ enabled, provider, cloudConsent: cloudConsentState.accepted }))
    const chatting = $derived(gate === 'chat')

    // A gate that opens while the rail is up (Ask Cmdr turned on here, cloud AI allowed in
    // Settings) loads the last thread, which `openRail` skipped behind the gate. Only on the
    // transition: at mount `openRail` already did it.
    let wasChatting: boolean | null = null
    $effect(() => {
        const now = chatting
        untrack(() => {
            if (wasChatting === false && now) void ensureThreadLoaded()
            wasChatting = now
        })
    })

    // Whether a provider is set up, and the footer's baseline: read while the rail is up, and
    // again on a thread switch or a turn's end (the memory slice and the budget may have moved).
    $effect(() => watchSlotStatus())
    let lastSeen: { id: number | null; streaming: boolean } | null = null
    $effect(() => {
        const now = { id: askCmdrState.conversationId, streaming: askCmdrState.streaming }
        untrack(() => {
            const turnEnded = lastSeen?.streaming === true && !now.streaming
            if (lastSeen !== null && (turnEnded || lastSeen.id !== now.id)) void refreshSlotStatus()
            lastSeen = now
        })
    })
    const gap = $derived(providerGap(slotStatusState.status))

    // The threads panel lives inside the rail, so it closes with it, search and all, and the rail
    // reopens on the chat. Every close path (⌘⌥A, the close button, the error boundary) unmounts
    // the rail, and the sessions slice can't hear `closeRail` (it calls the trigger, never back).
    onDestroy(dismissSessions)

    let listElement = $state<HTMLDivElement | null>(null)
    // Whether the user was near the bottom before the last content change. Streaming
    // appends scroll to follow; a "load earlier" prepend (user scrolled up) does not.
    let wasNearBottom = true

    function onListScroll(): void {
        const el = listElement
        if (el) wasNearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 120
    }

    // Keep the newest message in view as the thread grows or a turn streams in (a round, a tool
    // line, prose, its stats), but only when the user is already at the bottom.
    const tail = $derived(contentSignature(askCmdrState.messages))
    $effect(() => {
        dependOn(tail)
        if (!wasNearBottom) return
        void tick().then(() => {
            if (listElement) listElement.scrollTop = listElement.scrollHeight
        })
    })

    // Prepend older history, holding the user's scroll position steady across the growth. The
    // last page takes the button away, so focus on it would fall to `<body>`: the rail keeps it.
    async function onLoadEarlier(): Promise<void> {
        const el = listElement
        const before = el ? el.scrollHeight : 0
        await loadOlderMessages()
        await tick()
        if (el) el.scrollTop += el.scrollHeight - before
        if (document.activeElement === document.body) railElement?.focus()
    }

    // The rail is focusable (tabindex -1, out of the tab order) so a click on its text or on a
    // button keeps focus in the rail: WebKit doesn't focus a clicked button, and with no
    // focusable ancestor it drops focus to `<body>`, where the keyboard reaches nothing.
    let railElement = $state<HTMLElement | null>(null)

    const newChatShortcut = $derived(newChatShortcutText())

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

<aside
    class="ask-cmdr-rail"
    style="width: {askCmdrState.width}px"
    aria-label={tString('askCmdr.title')}
    tabindex="-1"
    bind:this={railElement}
    onfocusin={markRailFocused}
>
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
            {#if chatting}
                <button type="button" class="icon-button" data-rail-action="threads" onclick={openSessions} aria-label={tString('askCmdr.threads.open')} use:tooltip={tString('askCmdr.threads.open')}>
                    <Icon name="messages-square" size={16} aria-hidden="true" />
                </button>
                <button
                    type="button"
                    class="icon-button"
                    data-rail-action="new-chat"
                    onclick={openNewChat}
                    aria-label={tString('askCmdr.newChat')}
                    use:tooltip={{ text: tString('askCmdr.newChat'), shortcut: newChatShortcut }}
                >
                    <Icon name="file-plus" size={16} aria-hidden="true" />
                </button>
            {/if}
            <button type="button" class="icon-button" onclick={closeRail} aria-label={tString('askCmdr.close')}>
                <Icon name="x" size={16} aria-hidden="true" />
            </button>
        </span>
    </header>

    <!-- Above whichever gate or chat follows, so with Ask Cmdr off both steps show at once. -->
    {#if gap}
        <AskCmdrProviderNotice {gap} />
    {/if}

    {#if gate === 'off' || gate === 'cloudOff'}
        <AskCmdrGate kind={gate} />
    {:else if chatting}
        <div class="rail-body" bind:this={listElement} onscroll={onListScroll}>
            {#if hasOlderMessages()}
                <button type="button" class="load-earlier" disabled={askCmdrState.loadingOlder} onclick={() => void onLoadEarlier()}>
                    {tString('askCmdr.loadEarlier')}
                </button>
            {/if}
            {#if askCmdrState.messages.length === 0}
                <div class="empty">
                    <span class="empty-glyph"><Icon name="sparkles" size={28} aria-hidden="true" /></span>
                    <p class="empty-title">{tString('askCmdr.empty.title')}</p>
                    <p class="empty-hint">{tString('askCmdr.empty.hint')}</p>
                </div>
            {:else}
                {#each askCmdrState.messages as message, index (index)}
                    <AskCmdrMessage {message} {revealPath} />
                {/each}
                {#if askCmdrState.streaming && askCmdrState.spendPausedMicros !== null}
                    <AskCmdrSpendPause
                        costMicros={askCmdrState.spendPausedMicros}
                        onKeepGoing={keepGoingPastSpend}
                        onStop={stopStreaming}
                    />
                {/if}
                {#if longChatNudge() !== null}
                    <div class="soft-cap">
                        <CloseButton
                            class="soft-cap-dismiss"
                            onclick={dismissLongChatNudge}
                            ariaLabel={tString('askCmdr.softCap.dismiss')}
                        />
                        <span>{tString('askCmdr.softCap.message')}</span>
                        <button type="button" class="soft-cap-action" onclick={openNewChat}>
                            {tString('askCmdr.softCap.action')}
                        </button>
                    </div>
                {/if}
            {/if}
        </div>

        <AskCmdrComposer {readSelection} />
        <AskCmdrCostFooter />

        {#if sessionsState.open}
            <AskCmdrSessions />
        {/if}
    {/if}
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

    /* Focus lands on the rail itself only from a click (it's out of the tab order), so it draws
       no ring: the region it marks is already the whole panel. */
    .ask-cmdr-rail:focus {
        outline: none;
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

    /* A 24-by-24 grab pad at the top of the edge (WCAG 2.5.8), over the header's padding and
       decorative glyph, the one stretch of this edge with no control or text to cover. */
    .resize-handle::after {
        content: '';
        position: absolute;
        top: 0;
        left: 0;
        width: var(--spacing-xl);
        height: var(--spacing-xl);
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

    .load-earlier {
        display: block;
        width: 100%;
        margin-bottom: var(--spacing-sm);
        padding: var(--spacing-xs);
        font: inherit;
        font-size: var(--font-size-sm);
        color: var(--color-accent-text);
        background: none;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
    }

    .load-earlier:disabled {
        opacity: 0.5;
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
        line-height: var(--font-line-height-prose);
    }

    .soft-cap {
        position: relative;
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

    /* The message keeps clear of the corner ×, which reads first in focus order. */
    .soft-cap > span {
        padding-right: var(--spacing-lg);
    }

    .soft-cap :global(.soft-cap-dismiss) {
        position: absolute;
        top: calc(var(--spacing-sm) - 3px);
        right: calc(var(--spacing-sm) - 3px);
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
