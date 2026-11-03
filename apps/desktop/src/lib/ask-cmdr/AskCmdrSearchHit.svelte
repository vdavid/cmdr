<!--
  One search result's content in the threads panel: the title (a wake thread's display title
  while it keeps the wake's own), how long ago the thread was last active, and an excerpt of
  its best-matching message with the matched words marked. The option element around it, the
  cursor, and the keys belong to `AskCmdrSessions.svelte`.

  ⚠️ The excerpt is thread text (the user's, the model's, file paths): untrusted. It renders as
  plain text runs, matched runs inside `<mark>`, never {@html}. The backend already split it
  (`agent/store/search.rs`, `SnippetPart`).
-->
<script lang="ts">
    import Icon from '#lib/ui/Icon.svelte'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatTimeAgo } from '#lib/intl/relative-time-format.js'
    import { formatLongDate } from '#lib/settings/format-utils.js'
    import { threadDisplayTitle } from './ask-cmdr-thread-title'
    import type { ConversationSearchHit } from '#lib/tauri-commands/index.js'

    interface Props {
        hit: ConversationSearchHit
        /** "Now" in unix seconds, one snapshot per result set, so every row ages against the same clock. */
        nowSecs: number
    }

    const { hit, nowSecs }: Props = $props()

    const title = $derived(threadDisplayTitle(hit))
    const exactDate = $derived(formatLongDate(new Date(hit.updatedAt * 1000)) ?? '')
    // An engine with no relative-time words shows the calendar date in the same slot.
    const age = $derived(formatTimeAgo(hit.updatedAt, nowSecs) ?? exactDate)
</script>

<span class="hit-heading">
    {#if hit.origin === 'notification'}
        <span class="started-by-agent" use:tooltip={tString('askCmdr.sessions.wakeThread')}>
            <Icon name="bot" size={12} aria-label={tString('askCmdr.sessions.wakeThread')} />
        </span>
    {/if}
    <span class="row-title">{title}</span>
    <span class="hit-age" use:tooltip={exactDate}>{age}</span>
</span>
{#if hit.snippet.length > 0}
    <span class="hit-snippet"
        >{#each hit.snippet as part, i (i)}{#if part.matched}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}</span
    >
{/if}
{#if hit.archived}
    <span class="archived-badge">{tString('askCmdr.sessions.archivedBadge')}</span>
{/if}

<style>
    .hit-heading {
        display: flex;
        align-items: baseline;
        gap: var(--spacing-xs);
        min-width: 0;
    }

    /* The glyph keeps its size while the title takes the squeeze. */
    .started-by-agent {
        display: flex;
        flex: none;
        align-self: center;
        color: var(--color-text-tertiary);
    }

    .row-title {
        flex: 1;
        min-width: 0;
        font-size: var(--font-size-sm);
        font-weight: 500;
        color: var(--color-text-primary);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .hit-age {
        flex: none;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        white-space: nowrap;
    }

    /* Two lines of the matching message, so the match reads in its sentence. */
    .hit-snippet {
        display: -webkit-box;
        -webkit-box-orient: vertical;
        -webkit-line-clamp: 2;
        line-clamp: 2;
        overflow: hidden;
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        overflow-wrap: break-word;
    }

    /* The palette's match mark: an accent underline, so the text keeps its contrast. */
    mark {
        color: var(--color-text-primary);
        font-weight: 600;
        background: none;
        text-decoration: underline;
        text-decoration-color: var(--color-accent);
        text-decoration-thickness: 2px;
        text-underline-offset: 2px;
    }

    .archived-badge {
        align-self: flex-start;
        padding: 0 var(--spacing-xxs);
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-xs);
    }
</style>
