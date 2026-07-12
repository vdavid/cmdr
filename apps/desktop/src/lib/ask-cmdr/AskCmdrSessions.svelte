<!--
  The Ask Cmdr sessions panel: it overlays the rail body to list past chats (recent
  first), search across them, and rename/archive/switch. Modeled on the operation-log
  dialog's list + paging (offset = list length). All chat titles and search snippets are
  filesystem-adjacent / model-adjacent text, so they render as plain {text}, never {@html}.
-->
<script lang="ts">
    import { tick } from 'svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import {
        chooseThread,
        clearSearch,
        closeSessions,
        isSearching,
        loadMoreSessions,
        renameConversation,
        sessionsState,
        setArchived,
        setSearchQuery,
        startNewChat,
        toggleShowArchived,
    } from './ask-cmdr-sessions.svelte'

    let editingId = $state<number | null>(null)
    let editTitle = $state('')
    let editInput = $state<HTMLInputElement | null>(null)

    async function startRename(id: number, title: string): Promise<void> {
        editingId = id
        editTitle = title
        await tick()
        editInput?.select()
    }

    async function commitRename(): Promise<void> {
        const id = editingId
        if (id === null) return
        editingId = null
        try {
            await renameConversation(id, editTitle)
        } catch {
            // A failed rename leaves the old title; the list already shows it.
        }
    }

    function cancelRename(): void {
        editingId = null
    }

    function onRenameKeydown(event: KeyboardEvent): void {
        if (event.key === 'Enter') {
            event.preventDefault()
            void commitRename()
        } else if (event.key === 'Escape') {
            event.preventDefault()
            cancelRename()
        }
    }
</script>

<div class="sessions">
    <header class="sessions-header">
        <button type="button" class="icon-button" onclick={closeSessions} aria-label={tString('askCmdr.sessions.back')}>
            <Icon name="arrow-left" size={16} aria-hidden="true" />
        </button>
        <span class="sessions-title">{tString('askCmdr.sessions.title')}</span>
        <button type="button" class="icon-button" onclick={startNewChat} aria-label={tString('askCmdr.newChat')}>
            <Icon name="file-plus" size={16} aria-hidden="true" />
        </button>
    </header>

    <div class="search-row">
        <Icon name="search" size={14} aria-hidden="true" />
        <input
            type="text"
            class="search-input"
            value={sessionsState.query}
            placeholder={tString('askCmdr.sessions.searchPlaceholder')}
            aria-label={tString('askCmdr.sessions.searchLabel')}
            oninput={(e) => { setSearchQuery(e.currentTarget.value); }}
        />
        {#if sessionsState.query.length > 0}
            <button type="button" class="icon-button small" onclick={clearSearch} aria-label={tString('askCmdr.close')}>
                <Icon name="x" size={14} aria-hidden="true" />
            </button>
        {/if}
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
                <ul class="list">
                    {#each sessionsState.hits as hit (hit.conversationId)}
                        <li>
                            <button type="button" class="row" onclick={() => void chooseThread(hit.conversationId)}>
                                <span class="row-title">{hit.title}</span>
                                {#if hit.snippet}
                                    <span class="row-snippet">{hit.snippet}</span>
                                {/if}
                            </button>
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
                    <li class="conversation">
                        {#if editingId === conversation.id}
                            <input
                                bind:this={editInput}
                                bind:value={editTitle}
                                type="text"
                                class="rename-input"
                                aria-label={tString('askCmdr.sessions.renameLabel')}
                                onkeydown={onRenameKeydown}
                                onblur={() => void commitRename()}
                            />
                        {:else}
                            <button type="button" class="row" onclick={() => void chooseThread(conversation.id)}>
                                <span class="row-title">{conversation.title}</span>
                                {#if conversation.archived}
                                    <span class="archived-badge">{tString('askCmdr.sessions.archivedBadge')}</span>
                                {/if}
                            </button>
                            <span class="row-actions">
                                <button
                                    type="button"
                                    class="icon-button small"
                                    onclick={() => void startRename(conversation.id, conversation.title)}
                                    aria-label={tString('askCmdr.sessions.rename')}
                                >
                                    <Icon name="pencil" size={13} aria-hidden="true" />
                                </button>
                                <button
                                    type="button"
                                    class="icon-button small"
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

    .search-row {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        margin: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-sm);
        color: var(--color-text-secondary);
        background: var(--color-bg-primary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
    }

    .search-input {
        flex: 1;
        min-width: 0;
        border: none;
        background: none;
        font: inherit;
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    .search-input:focus-visible {
        outline: none;
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

    .row-title {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .row-snippet {
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
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

    .rename-input {
        flex: 1;
        min-width: 0;
        margin: var(--spacing-xxs) 0;
        padding: var(--spacing-xs) var(--spacing-sm);
        font: inherit;
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        background: var(--color-bg-primary);
        border: 1px solid var(--color-accent);
        border-radius: var(--radius-sm);
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
