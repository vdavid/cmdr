<!--
  The Ask Cmdr sessions panel: it overlays the rail body to list past chats (recent
  first), search across them, and rename/archive/switch. Modeled on the operation-log
  dialog's list + paging (offset = list length). All chat titles and search snippets are
  filesystem-adjacent / model-adjacent text, so they render as plain {text}, never {@html}.

  Search results are a combobox over a listbox, the command palette's shape: focus stays in
  the search field, ↑/↓ move the cursor (`aria-activedescendant`), Enter opens the cursor's
  thread, and Escape clears, then closes (`escapeSessions`). Ranking and excerpts are the
  backend's (`agent/store/search.rs`).
-->
<script lang="ts">
    import { tick } from 'svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import TextInput from '#lib/ui/TextInput.svelte'
    import Spinner from '#lib/ui/Spinner.svelte'
    import LiveAnnouncer from '#lib/ui/LiveAnnouncer.svelte'
    import { listCursorMoveOf, nextListCursor } from '#lib/ui/list-cursor.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import { dependOn } from '#lib/utils/reactivity.js'
    import AskCmdrSearchHit from './AskCmdrSearchHit.svelte'
    import { threadDisplayTitle } from './ask-cmdr-thread-title'
    import { newChatShortcutText } from './ask-cmdr-trigger.svelte'
    import {
        chooseThread,
        clearSearch,
        closeSessions,
        escapeSessions,
        isSearching,
        loadMoreSessions,
        renameConversation,
        sessionsState,
        setArchived,
        setSearchQuery,
        startNewChat,
        toggleShowArchived,
    } from './ask-cmdr-sessions.svelte'
    import type { ConversationOrigin } from '#lib/ipc/bindings.js'

    /** True for a thread the agent opened for itself, which earns the row a glyph. The
     *  reserved quiet-wakes ledger row never reaches either list, so `notification` is the
     *  only token that can get here — but branch on the token, never on "not null", so a
     *  later origin doesn't silently inherit the wake's mark. */
    const startedByTheAgent = (origin: ConversationOrigin | null): boolean => origin === 'notification'

    let editingId = $state<number | null>(null)
    let editTitle = $state('')
    /** What the rename field opened with: committing it unchanged renames nothing. */
    let editStartTitle = ''
    let editInput = $state<HTMLInputElement | undefined>()

    async function startRename(id: number, title: string): Promise<void> {
        editingId = id
        editTitle = title
        editStartTitle = title
        await tick()
        editInput?.select()
    }

    async function commitRename(): Promise<void> {
        const id = editingId
        if (id === null) return
        editingId = null
        // A wake thread opens the field on its display title ("Activity in ~/tmp"); Enter on it
        // untouched would freeze that sentence into the store in today's language.
        if (editTitle.trim() === editStartTitle.trim()) return
        await renameConversation(id, editTitle)
    }

    // ── Search results: the cursor and its keys ──────────────────────────────────

    const idBase = $props.id()
    const resultsId = `${idBase}-results`
    const optionId = (index: number): string => `${idBase}-hit-${String(index)}`

    let cursorIndex = $state(0)
    /** One "now" per result set, so every row's age is measured against the same clock. */
    let nowSecs = $state(Math.floor(Date.now() / 1000))
    let resultsElement = $state<HTMLUListElement | undefined>()

    // A new result set puts the cursor on its best match, so Enter in the field opens it.
    $effect(() => {
        dependOn(sessionsState.hits)
        cursorIndex = 0
        nowSecs = Math.floor(Date.now() / 1000)
    })

    const showingHits = $derived(isSearching() && sessionsState.hits.length > 0)
    const activeDescendant = $derived(showingHits ? optionId(cursorIndex) : undefined)

    async function moveCursorTo(index: number, focusOption: boolean): Promise<void> {
        cursorIndex = index
        await tick()
        const option = document.getElementById(optionId(index))
        option?.scrollIntoView({ block: 'nearest' })
        if (focusOption) option?.focus({ preventScroll: true })
    }

    /** ↑/↓ (wrapping), PageUp/PageDown, and Enter, from the field or from a focused result. */
    function onResultsKeydown(event: KeyboardEvent, fromOption: boolean): void {
        if (!showingHits || event.isComposing) return
        const count = sessionsState.hits.length
        if (event.key === 'Enter' && !event.altKey && !event.metaKey && !event.ctrlKey && !event.shiftKey) {
            const hit = sessionsState.hits.at(cursorIndex)
            if (!hit) return
            event.preventDefault()
            void chooseThread(hit.conversationId)
            return
        }
        const move = listCursorMoveOf(event, { textField: !fromOption })
        if (!move) return
        const next = nextListCursor(move, { count, from: cursorIndex, visibleRows: visibleResultRows(), wrap: true })
        if (next < 0) return
        event.preventDefault()
        void moveCursorTo(next, fromOption)
    }

    /** How many result rows fit on screen, for PageUp/PageDown. */
    function visibleResultRows(): number {
        const list = resultsElement
        const first = list?.querySelector<HTMLElement>('[role="option"]')
        const scroller = list?.parentElement
        if (!first || !scroller || first.offsetHeight === 0) return 1
        return Math.max(1, Math.floor(scroller.clientHeight / first.offsetHeight))
    }

    // How many chats matched, said once a search settles: the count is otherwise visible only.
    let announcer = $state<{ announce: (message: string) => void } | undefined>()
    $effect(() => {
        dependOn(sessionsState.hits)
        if (sessionsState.searching || !isSearching()) return
        const count = sessionsState.hits.length
        announcer?.announce(
            count === 0
                ? tString('askCmdr.sessions.noResults')
                : tString('askCmdr.sessions.resultCount', { count, countText: formatInteger(count) }),
        )
    })

    function cancelRename(): void {
        editingId = null
    }

    let sessionsElement = $state<HTMLDivElement | null>(null)

    // The rename field leaves the page once it's done, taking focus with it: a keyboard rename
    // hands focus to its row, so it doesn't fall to `<body>`. A blur-commit (a click elsewhere)
    // leaves focus where the click put it.
    async function focusRow(id: number): Promise<void> {
        await tick()
        sessionsElement?.querySelector<HTMLButtonElement>(`[data-conversation-row="${String(id)}"]`)?.focus()
    }

    function onRenameKeydown(event: KeyboardEvent): void {
        const id = editingId
        if (event.key === 'Enter') {
            event.preventDefault()
            void commitRename()
            if (id !== null) void focusRow(id)
        } else if (event.key === 'Escape') {
            event.preventDefault()
            cancelRename()
            if (id !== null) void focusRow(id)
        }
    }

    // Escape anywhere in the panel steps back (`escapeSessions`). The rename field's own Escape
    // cancels the rename and claims the key, so that press stays a cancel.
    function onPanelKeydown(event: KeyboardEvent): void {
        if (event.key !== 'Escape' || event.defaultPrevented || event.isComposing) return
        event.preventDefault()
        escapeSessions()
    }

    const newChatShortcut = $derived(newChatShortcutText())
</script>

<!-- The keydown only catches Escape bubbling up from the panel's own focusable controls, so the
     container needs no role of its own. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="sessions" bind:this={sessionsElement} onkeydown={onPanelKeydown}>
    <header class="sessions-header">
        <button type="button" class="icon-button" onclick={closeSessions} aria-label={tString('askCmdr.sessions.back')}>
            <Icon name="arrow-left" size={16} aria-hidden="true" />
        </button>
        <span class="sessions-title">{tString('askCmdr.sessions.title')}</span>
        <button
            type="button"
            class="icon-button"
            onclick={startNewChat}
            aria-label={tString('askCmdr.newChat')}
            use:tooltip={{ text: tString('askCmdr.newChat'), shortcut: newChatShortcut }}
        >
            <Icon name="file-plus" size={16} aria-hidden="true" />
        </button>
    </header>

    <LiveAnnouncer bind:this={announcer} />

    <div class="search-row">
        <TextInput
            type="search"
            radius="md"
            leadingIcon="search"
            value={sessionsState.query}
            placeholder={tString('askCmdr.sessions.searchPlaceholder')}
            ariaLabel={tString('askCmdr.sessions.searchLabel')}
            role="combobox"
            aria-controls={resultsId}
            aria-expanded={showingHits}
            aria-autocomplete="list"
            aria-activedescendant={activeDescendant}
            oninput={(e: Event) => {
                if (e.currentTarget instanceof HTMLInputElement) setSearchQuery(e.currentTarget.value)
            }}
            onkeydown={(e: KeyboardEvent) => {
                onResultsKeydown(e, false)
            }}
        >
            {#snippet trailing()}
                {#if sessionsState.query.length > 0}
                    <button
                        type="button"
                        class="icon-button clear"
                        onclick={clearSearch}
                        aria-label={tString('askCmdr.sessions.clearSearch')}
                        use:tooltip={tString('askCmdr.sessions.clearSearch')}
                    >
                        <Icon name="x" size={14} aria-hidden="true" />
                    </button>
                {/if}
            {/snippet}
        </TextInput>
    </div>

    {#if !isSearching()}
        <button type="button" class="archived-toggle" onclick={toggleShowArchived}>
            <Icon name="archive" size={13} aria-hidden="true" />
            <span>
                {sessionsState.showArchived
                    ? tString('askCmdr.sessions.hideArchived')
                    : tString('askCmdr.sessions.showArchived')}
            </span>
        </button>
    {/if}

    <div class="sessions-body">
        {#if isSearching()}
            {#if sessionsState.searching && sessionsState.hits.length === 0}
                <div class="notice"><Spinner size="sm" /><span>{tString('askCmdr.sessions.searching')}</span></div>
            {:else if sessionsState.hits.length === 0}
                <p class="notice">{tString('askCmdr.sessions.noResults')}</p>
            {:else}
                <!-- Keys live on the search field (↑ / ↓ / Enter over `aria-activedescendant`); a
                     result focused by Tab answers the same keys. -->
                <ul
                    class="list"
                    id={resultsId}
                    bind:this={resultsElement}
                    role="listbox"
                    aria-label={tString('askCmdr.sessions.resultsLabel')}
                >
                    {#each sessionsState.hits as hit, index (hit.conversationId)}
                        <li
                            id={optionId(index)}
                            class="row hit"
                            class:is-under-cursor={index === cursorIndex}
                            role="option"
                            aria-selected={index === cursorIndex}
                            tabindex={index === cursorIndex ? 0 : -1}
                            data-conversation-hit={hit.conversationId}
                            onclick={() => void chooseThread(hit.conversationId)}
                            onmouseenter={() => {
                                cursorIndex = index
                            }}
                            onkeydown={(e: KeyboardEvent) => {
                                onResultsKeydown(e, true)
                            }}
                        >
                            <AskCmdrSearchHit {hit} {nowSecs} />
                        </li>
                    {/each}
                </ul>
            {/if}
        {:else if sessionsState.loading && sessionsState.conversations.length === 0}
            <div class="notice"><Spinner size="sm" /></div>
        {:else if sessionsState.conversations.length === 0}
            <p class="notice">
                {sessionsState.showArchived
                    ? tString('askCmdr.sessions.emptyArchived')
                    : tString('askCmdr.sessions.empty')}
            </p>
        {:else}
            <ul class="list">
                {#each sessionsState.conversations as conversation (conversation.id)}
                    {@const title = threadDisplayTitle(conversation)}
                    <li class="conversation">
                        {#if editingId === conversation.id}
                            <TextInput
                                bind:inputElement={editInput}
                                bind:value={editTitle}
                                radius="sm"
                                containerStyle="flex: 1; min-width: 0; margin: var(--spacing-xxs) 0"
                                ariaLabel={tString('askCmdr.sessions.renameLabel')}
                                onkeydown={onRenameKeydown}
                                onblur={() => void commitRename()}
                            />
                        {:else}
                            <button
                                type="button"
                                class="row"
                                data-conversation-row={conversation.id}
                                onclick={() => void chooseThread(conversation.id)}
                            >
                                <span class="row-heading">
                                    {#if startedByTheAgent(conversation.origin)}
                                        <!-- Nobody typed the first message in this one. The
                                             same glyph the status corner wears while a wake
                                             thinks, so the two read as one feature. -->
                                        <span
                                            class="started-by-agent"
                                            use:tooltip={tString('askCmdr.sessions.wakeThread')}
                                        >
                                            <Icon
                                                name="bot"
                                                size={12}
                                                aria-label={tString('askCmdr.sessions.wakeThread')}
                                            />
                                        </span>
                                    {/if}
                                    <span class="row-title" use:tooltip={{ text: title, overflowOnly: true }}
                                        >{title}</span
                                    >
                                </span>
                                {#if conversation.archived}
                                    <span class="archived-badge">{tString('askCmdr.sessions.archivedBadge')}</span>
                                {/if}
                            </button>
                            <span class="row-actions">
                                <button
                                    type="button"
                                    class="icon-button small"
                                    data-row-action="rename"
                                    onclick={() => void startRename(conversation.id, title)}
                                    aria-label={tString('askCmdr.sessions.rename')}
                                >
                                    <Icon name="pencil" size={13} aria-hidden="true" />
                                </button>
                                <button
                                    type="button"
                                    class="icon-button small"
                                    data-row-action={conversation.archived ? 'unarchive' : 'archive'}
                                    onclick={() => void setArchived(conversation.id, !conversation.archived)}
                                    aria-label={conversation.archived
                                        ? tString('askCmdr.sessions.unarchive')
                                        : tString('askCmdr.sessions.archive')}
                                >
                                    <Icon name={conversation.archived ? 'archive-restore' : 'archive'} size={13} aria-hidden="true" />
                                </button>
                            </span>
                        {/if}
                    </li>
                {/each}
            </ul>
            {#if sessionsState.hasMore}
                <button
                    type="button"
                    class="load-more"
                    disabled={sessionsState.loadingMore}
                    onclick={() => void loadMoreSessions()}
                >
                    {tString('askCmdr.sessions.loadMore')}
                </button>
            {/if}
        {/if}
    </div>
</div>

<style>
    .sessions {
        position: absolute;
        inset: 0;
        z-index: var(--z-sticky);
        display: flex;
        flex-direction: column;
        min-height: 0;
        background: var(--color-bg-secondary);
    }

    .sessions-header {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-sm);
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .sessions-title {
        flex: 1;
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .icon-button {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 28px;
        height: 28px;
        flex: none;
        border: none;
        background: none;
        color: var(--color-text-secondary);
        border-radius: var(--radius-sm);
    }

    .icon-button.small {
        width: 24px;
        height: 24px;
    }

    .icon-button:hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    /* Only spacing: the field's frame, border, and focus ring are `TextInput`'s (`app.css` §
       "Text fields"). A border here once drew a second box around the field's own. */
    .search-row {
        margin: var(--spacing-sm);
    }

    /* The clear button sizes to the text line, like every trailing field control. */
    .icon-button.clear {
        width: 20px;
        height: 20px;
    }

    .archived-toggle {
        display: flex;
        align-items: center;
        gap: var(--spacing-xxs);
        margin: 0 var(--spacing-sm) var(--spacing-xs);
        padding: var(--spacing-xxs) var(--spacing-xs);
        font: inherit;
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        background: none;
        border: none;
        border-radius: var(--radius-sm);
        align-self: flex-start;
    }

    .archived-toggle:hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    .sessions-body {
        flex: 1;
        min-height: 0;
        overflow-y: auto;
        padding: 0 var(--spacing-sm) var(--spacing-sm);
    }

    .notice {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-xs);
        margin-top: var(--spacing-lg);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .list {
        list-style: none;
        margin: 0;
        padding: 0;
    }

    .conversation {
        display: flex;
        align-items: center;
        gap: var(--spacing-xxs);
        border-radius: var(--radius-sm);
    }

    .conversation:hover {
        background: var(--color-bg-tertiary);
    }

    .row {
        flex: 1;
        min-width: 0;
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        padding: var(--spacing-xs) var(--spacing-sm);
        text-align: left;
        background: none;
        border: none;
        border-radius: var(--radius-sm);
    }

    .row-heading {
        display: flex;
        align-items: center;
        gap: var(--spacing-xxs);
        min-width: 0;
    }

    /* The glyph keeps its size while the title takes the squeeze: the title ellipsizes, and a
       shrinking mark would be the first thing to become unreadable in a narrow rail. */
    .started-by-agent {
        display: flex;
        flex: none;
        color: var(--color-text-tertiary);
    }

    .row-title {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    /* A search result: two or three lines (title and age, the excerpt, a badge), the palette's
       accent tint under the cursor, which the pointer moves too. */
    .hit {
        gap: var(--spacing-xxs);
        padding: var(--spacing-sm);
    }

    .hit.is-under-cursor {
        background: var(--color-accent-subtle);
    }

    .hit:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
    }

    .archived-badge {
        align-self: flex-start;
        margin-top: var(--spacing-xxs);
        padding: 0 var(--spacing-xxs);
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-xs);
    }

    .row-actions {
        display: flex;
        gap: var(--spacing-xxs);
        padding-right: var(--spacing-xxs);
    }

    .load-more {
        display: block;
        width: 100%;
        margin-top: var(--spacing-xs);
        padding: var(--spacing-xs);
        font: inherit;
        font-size: var(--font-size-sm);
        color: var(--color-accent-text);
        background: none;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
    }

    .load-more:disabled {
        opacity: 0.5;
    }
</style>
