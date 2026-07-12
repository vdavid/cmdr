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
    import { errorMessage } from './ask-cmdr-labels'
    import { renderAssistantMarkdown } from './ask-cmdr-markdown'
    import AskCmdrToolLine from './AskCmdrToolLine.svelte'
    import type { RailMessage } from './ask-cmdr-trigger.svelte'

    interface Props {
        message: RailMessage
    }
    const { message }: Props = $props()
</script>

{#if message.kind === 'user'}
    <div class="msg user">
        <div class="bubble">{message.text}</div>
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
        {#if message.thinking}
            <div class="thinking" role="status">
                <Spinner size="sm" />
                <span>{tString('askCmdr.thinking')}</span>
            </div>
        {/if}
        {#if message.text}
            <div class="prose" aria-live="polite">
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
        <span>{errorMessage(message.errorKind)}</span>
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

    .user .bubble {
        max-width: 85%;
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
        gap: var(--spacing-xxs);
        margin-bottom: var(--spacing-xs);
    }

    .thinking {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        margin-bottom: var(--spacing-xs);
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
    }

    .prose {
        font-size: var(--font-size-sm);
        line-height: 1.55;
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
</style>
