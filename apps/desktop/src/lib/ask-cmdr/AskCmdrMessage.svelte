<!--
  One rendered thread item: a user bubble, an assistant turn (tool lines + a "thinking…"
  indicator + markdown-lite prose with a streaming cursor), or a typed failure notice.

  Assistant prose is the XSS boundary: it's untrusted model text, so it goes through
  `renderAssistantMarkdown` (HTML-entity escape + snarkdown) before {@html}. User text and
  the error copy render through Svelte's auto-escaping interpolation, never {@html}.
-->
<script lang="ts">
    import Icon from '$lib/ui/Icon.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { errorMessage, undoSkipMessage } from './ask-cmdr-labels'
    import { renderAssistantMarkdown } from './ask-cmdr-markdown'
    import AskCmdrToolLine from './AskCmdrToolLine.svelte'
    import AskCmdrAttachmentChip from './AskCmdrAttachmentChip.svelte'
    import AskCmdrWakeDigest from './AskCmdrWakeDigest.svelte'
    import AskCmdrProposalDecisions from './AskCmdrProposalDecisions.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { undoRename, type RailMessage } from './ask-cmdr-trigger.svelte'

    interface Props {
        message: RailMessage
    }
    const { message }: Props = $props()

    /** A count as both the preformatted string the sentence drops in and the raw
     *  integer that picks the plural form (the catalogs' `*Text` convention). */
    function undoCounts(count: number): { countText: string; count: number } {
        return { countText: formatInteger(count), count }
    }

    /** Undo's accessible name says WHAT it would reverse: "Undo" alone tells a screen
     *  reader nothing once focus arrives without the sentence beside it. */
    const undoLabel = $derived(
        message.kind === 'renameApplied'
            ? tString('askCmdr.renameUndo.undoLabel', undoCounts(message.fileCount))
            : '',
    )
    /** The lines explaining what an undo left behind: one per reason it recorded, naming
     *  the file when a reason applies to just one. Whatever no reason accounts for is still
     *  said by class, so the skipped COUNT is always fully reported — a missing reason must
     *  never quietly shrink what the line admits to. */
    const skipReasonLines = $derived.by(() => {
        if (message.kind !== 'renameApplied' || message.undo.status !== 'partial') return []
        const { skips, skipped } = message.undo
        const lines: string[] = []
        let explained = 0
        for (const group of skips) {
            const line = undoSkipMessage(group)
            if (line === null) continue
            lines.push(line)
            explained += group.count
        }
        const unexplained = Math.max(skipped - explained, 0)
        if (unexplained > 0) lines.push(tString('askCmdr.renameUndo.skipped', undoCounts(unexplained)))
        return lines
    })

    const undoJobLabel = $derived(
        message.kind === 'renameApplied'
            ? tString('askCmdr.renameUndo.undoJobLabel', {
                  filesText: formatInteger(message.jobFileCount),
                  files: message.jobFileCount,
                  batches: message.jobOperationIds.length,
              })
            : '',
    )
</script>

{#if message.kind === 'user'}
    <div class="msg user">
        <div class="user-stack">
            <div class="bubble" data-text-region>{message.text}</div>
            {#if message.attachments.length > 0}
                <div class="user-attachments">
                    {#each message.attachments as attachment (attachment.path)}
                        <AskCmdrAttachmentChip {attachment} />
                    {/each}
                </div>
            {/if}
        </div>
    </div>
{:else if message.kind === 'wakeDigest'}
    <!-- The opener of a thread the agent started for itself. Full width rather than a
         right-aligned bubble: nobody typed it, so it doesn't belong on the user's side. -->
    <div class="msg">
        <AskCmdrWakeDigest folders={message.folders} rollups={message.rollups} />
    </div>
{:else if message.kind === 'proposalDecisions'}
    <!-- What the user answered about a suggestion. Full width rather than a right-aligned
         bubble: it is a record of a decision, not something anybody typed. -->
    <div class="msg">
        <AskCmdrProposalDecisions decisions={message.decisions} />
    </div>
{:else if message.kind === 'assistant'}
    <div class="msg">
        {#if message.tools.length > 0}
            <div class="tools">
                {#each message.tools as tool (tool.callId)}
                    <AskCmdrToolLine {tool} />
                {/each}
            </div>
        {/if}
        {#if message.thinking || message.stalled}
            <div class="status-line" role="status">
                <span class="status-glyph"><Spinner size="sm" /></span>
                <span>{message.stalled ? tString('askCmdr.stalled') : tString('askCmdr.thinking')}</span>
            </div>
        {/if}
        {#if message.text}
            <div class="prose" data-text-region aria-live="polite">
                <!-- eslint-disable-next-line svelte/no-at-html-tags -- untrusted model text is HTML-entity-escaped (escapeForMarkdownLite) before snarkdown inside renderAssistantMarkdown; this is the XSS boundary. -->
                {@html renderAssistantMarkdown(message.text)}{#if message.streaming}<span
                        class="cursor"
                        aria-hidden="true"
                    ></span>{/if}
            </div>
        {/if}
    </div>
{:else if message.kind === 'error'}
    <div class="msg error" role="status">
        <Icon name="triangle-alert" size={14} aria-hidden="true" />
        <div class="error-stack">
            <span>{errorMessage(message.errorKind)}</span>
            {#if message.detail}
                <!-- The provider's own wording, so the user sees what to fix. Plain {text}
                     (Svelte auto-escapes) — never {@html}; this string is untrusted. -->
                <span class="error-detail">{message.detail}</span>
            {/if}
        </div>
    </div>
{:else if message.kind === 'modelChange'}
    <!-- A timeline line, not a bubble: the thread switched models here. The model name
         renders as plain {text} (Svelte auto-escapes), never {@html}. -->
    <div class="msg model-change" role="status">
        {tString('askCmdr.event.modelChanged', { model: message.model })}
    </div>
{:else if message.kind === 'contextTrimmed'}
    <!-- A timeline line: older lookups left the model's context so this turn fit its budget,
         so the reply that follows saw less than the whole chat. -->
    <div class="msg model-change" role="status">
        {tString('askCmdr.event.contextTrimmed', { count: message.count })}
    </div>
{:else if message.kind === 'renameApplied'}
    <!-- The safety net that fires AFTER the names land: the user only finds out a name
         is wrong once they see the result. `role="status"` so a screen reader hears the
         outcome without the focus moving. -->
    <div class="msg rename-applied" role="status">
        {#if message.undo.status === 'undoing'}
            <span class="status-glyph"><Spinner size="sm" /></span>
            <span>{tString('askCmdr.renameUndo.undoing')}</span>
        {:else if message.undo.status === 'undone'}
            <span>{tString('askCmdr.renameUndo.undone', undoCounts(message.undo.restored))}</span>
        {:else if message.undo.status === 'partial'}
            <!-- Loud about what stayed behind: undo never overwrites, so a file that
                 changed since keeps its new name and this says so. -->
            <div class="rename-lines">
                <span>{tString('askCmdr.renameUndo.partial', undoCounts(message.undo.restored))}</span>
                <!-- One line per REASON, naming the file when a reason applies to just one.
                     `skipReasonLines` is empty when nothing was skipped, and falls back to
                     the reason-class line when the backend recorded no reason (a batch
                     undone before the reason column existed). -->
                {#each skipReasonLines as reasonLine, index (index)}
                    <span class="rename-note">{reasonLine}</span>
                {/each}
                {#if message.undo.refusedBatches > 0}
                    <span class="rename-note">
                        {tString('askCmdr.renameUndo.refusedBatches', undoCounts(message.undo.refusedBatches))}
                    </span>
                {/if}
            </div>
        {:else if message.undo.status === 'unavailable'}
            <span class="rename-note">{tString('askCmdr.renameUndo.unavailable')}</span>
        {:else}
            <span>{tString('askCmdr.renameUndo.applied', undoCounts(message.fileCount))}</span>
            <button type="button" class="undo" aria-label={undoLabel} onclick={() => void undoRename(message)}>
                {tString('askCmdr.renameUndo.undo')}
            </button>
            {#if message.jobOperationIds.length > 1}
                <!-- One run, several approved batches: undo them all. The backend reverses
                     newest batch first, the only order that survives a batch reusing a
                     name an earlier one freed. -->
                <button
                    type="button"
                    class="undo"
                    aria-label={undoJobLabel}
                    onclick={() => void undoRename(message, 'job')}
                >
                    {tString('askCmdr.renameUndo.undoJob', undoCounts(message.jobOperationIds.length))}
                </button>
            {/if}
        {/if}
    </div>
{/if}

<style>
    .msg {
        margin-bottom: var(--spacing-md);
    }

    .msg.user {
        display: flex;
        justify-content: flex-end;
    }

    .user-stack {
        display: flex;
        flex-direction: column;
        align-items: flex-end;
        gap: var(--spacing-xxs);
        max-width: 85%;
    }

    .user-attachments {
        display: flex;
        flex-wrap: wrap;
        justify-content: flex-end;
        gap: var(--spacing-xxs);
    }

    .user .bubble {
        max-width: 100%;
        padding: var(--spacing-xs) var(--spacing-sm);
        background: var(--color-accent-subtle);
        color: var(--color-text-primary);
        border-radius: var(--radius-md);
        font-size: var(--font-size-sm);
        white-space: pre-wrap;
        word-break: break-word;
    }

    .tools {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        margin-bottom: var(--spacing-xs);
    }

    .status-line {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        min-height: 28px;
        padding: var(--spacing-xxs) var(--spacing-xs);
        margin-bottom: var(--spacing-xs);
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
    }

    .status-glyph {
        display: flex;
        flex: none;
        width: 16px;
        justify-content: center;
    }

    .rename-applied {
        display: flex;
        flex-wrap: wrap;
        align-items: baseline;
        gap: var(--spacing-xs);
        padding: var(--spacing-xs) var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-md);
    }

    .rename-lines {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    .rename-note {
        color: var(--color-text-tertiary);
    }

    /* A link-shaped action, not a button: it sits inside a sentence. `--color-accent-text`
       rather than `--color-accent`, which has too little contrast as foreground. */
    .undo {
        padding: 0;
        font: inherit;
        color: var(--color-accent-text);
        background: none;
        border: none;
        text-decoration: underline;
    }

    .undo:hover {
        text-decoration-thickness: 2px;
    }

    .bubble,
    .prose {
        user-select: text;
        -webkit-user-select: text;
    }


    .prose {
        font-size: var(--font-size-sm);
        line-height: var(--font-line-height-prose);
        color: var(--color-text-primary);
        word-break: break-word;
    }

    /* snarkdown output: tighten the default block margins to the rail's rhythm. */
    .prose :global(p) {
        margin: 0 0 var(--spacing-xs);
    }

    .prose :global(p:last-child) {
        margin-bottom: 0;
    }

    .prose :global(ul),
    .prose :global(ol) {
        margin: 0 0 var(--spacing-xs);
        padding-left: var(--spacing-lg);
    }

    .prose :global(code) {
        padding: 0.1em 0.3em;
        font-family: var(--font-mono);
        font-size: 0.9em;
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-xs);
    }

    .cursor {
        display: inline-block;
        width: 0.5em;
        height: 1em;
        margin-left: 1px;
        vertical-align: text-bottom;
        background: var(--color-accent);
        animation: blink 1s step-start infinite;
    }


    @media (prefers-reduced-motion: reduce) {
        .cursor {
            animation: none;
        }
    }

    @keyframes blink {
        50% {
            opacity: 0;
        }
    }

    .msg.error {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xs) var(--spacing-sm);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-md);
    }

    .error-stack {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        min-width: 0;
    }

    .error-detail {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        overflow-wrap: anywhere;
    }

    .msg.model-change {
        text-align: center;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        overflow-wrap: anywhere;
    }
</style>
