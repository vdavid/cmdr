<!--
  One collapsible "looked at X" line for a tool call the assistant made. Collapsed shows a
  status glyph + the localized action label; expanding reveals the full path (rendered as
  ESCAPED plain text via Svelte's default interpolation, never {@html} — a filename is
  attacker-controlled).
-->
<script lang="ts">
    import Icon from '$lib/ui/Icon.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { toolLabel, toolRefusedLabel } from './ask-cmdr-labels'
    import type { RailToolCall } from './ask-cmdr-trigger.svelte'

    interface Props {
        tool: RailToolCall
    }
    const { tool }: Props = $props()

    let expanded = $state(false)

    const label = $derived(
        tool.running ? toolLabel(tool.tool, true) : tool.ok ? toolLabel(tool.tool, false) : toolRefusedLabel(),
    )
    const hasDetail = $derived(tool.path !== null)
</script>

<div class="tool-line" role="status" aria-busy={tool.running}>
    <button
        type="button"
        class="tool-toggle"
        aria-expanded={hasDetail ? expanded : undefined}
        disabled={!hasDetail}
        onclick={() => (expanded = !expanded)}
    >
        <span class="glyph">
            {#if tool.running}
                <Spinner size="sm" />
            {:else if tool.ok}
                <Icon name="circle-check" size={13} aria-hidden="true" />
            {:else}
                <Icon name="circle-x" size={13} aria-hidden="true" />
            {/if}
        </span>
        <span class="label">{label}</span>
        {#if tool.path}
            <span class="path">{tool.path}</span>
            <span class="chevron">
                <Icon name={expanded ? 'chevron-down' : 'chevron-right'} size={13} aria-hidden="true" />
            </span>
        {/if}
    </button>
    {#if expanded && tool.path}
        <div class="detail">{tool.path}</div>
    {/if}
</div>

<style>
    .tool-line {
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
    }

    .tool-toggle {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        width: 100%;
        min-height: 28px;
        padding: var(--spacing-xxs) var(--spacing-xs);
        border: none;
        background: none;
        color: inherit;
        font: inherit;
        text-align: left;
        border-radius: var(--radius-xs);
    }

    .tool-toggle:not(:disabled):hover {
        background: var(--color-bg-tertiary);
    }

    .glyph {
        display: flex;
        width: 16px;
        justify-content: center;
        flex: none;
        color: var(--color-text-tertiary);
    }

    .label {
        flex: none;
    }

    .path {
        flex: 1;
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        color: var(--color-text-tertiary);
        font-family: var(--font-mono);
    }

    .chevron {
        display: flex;
        flex: none;
        color: var(--color-text-tertiary);
    }

    .detail {
        padding: var(--spacing-xxs) var(--spacing-xs) var(--spacing-xs) calc(var(--spacing-xs) + 20px);
        color: var(--color-text-tertiary);
        font-family: var(--font-mono);
        word-break: break-all;
    }
</style>
