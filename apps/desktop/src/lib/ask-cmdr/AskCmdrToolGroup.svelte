<!--
  A run of tool lines with no narration between them, under one quiet header that says what they
  looked at ("Worked through: 6 folders, 2 searches"). Collapsed, the group is that one line.

  Open state lives in `ask-cmdr-tool-groups.svelte.ts`, keyed by the group's first line, never
  here: the rail keys messages by index, so local state would jump on a prepend. The header is NOT
  a live region; the bubble's one polite status announces a settled group.
-->
<script lang="ts">
    import Icon from '#lib/ui/Icon.svelte'
    import { groupKey, type DisplayLine } from './ask-cmdr-rounds'
    import { groupSummaryText } from './ask-cmdr-tool-group-summary'
    import { isGroupOpen, setGroupOpen } from './ask-cmdr-tool-groups.svelte'
    import type { RevealPath } from './ask-cmdr-tool-line'
    import AskCmdrToolLine from './AskCmdrToolLine.svelte'

    interface Props {
        lines: DisplayLine[]
        /** Whether the group's turn is still streaming: an untouched group stays open until it ends. */
        live: boolean
        revealPath: RevealPath
    }
    const { lines, live, revealPath }: Props = $props()

    const uid = $props.id()
    const bodyId = `${uid}-body`
    const key = $derived(groupKey(lines))
    const running = $derived(lines.some((line) => line.call.running))
    const open = $derived(isGroupOpen(key, { live, running }))
    const summary = $derived(groupSummaryText(lines.map((line) => line.call)))

    let toggleEl: HTMLButtonElement | undefined = $state()
    let bodyEl: HTMLDivElement | undefined = $state()

    // A collapse the turn's end applies mustn't strand focus inside the closing body: before the
    // DOM drops it, focus moves to the toggle.
    $effect.pre(() => {
        if (open || !bodyEl || !toggleEl) return
        if (bodyEl.contains(document.activeElement)) toggleEl.focus()
    })

    function toggle(): void {
        if (key !== null) setGroupOpen(key, !open)
    }
</script>

<div class="tool-group">
    <button
        bind:this={toggleEl}
        type="button"
        class="group-toggle"
        aria-expanded={open}
        aria-controls={open ? bodyId : undefined}
        disabled={key === null}
        onclick={toggle}
    >
        <span class="chevron">
            <Icon name={open ? 'chevron-down' : 'chevron-right'} size={13} aria-hidden="true" />
        </span>
        <span class="summary" data-find-region>{summary}</span>
    </button>
    {#if open}
        <div class="group-body" id={bodyId} bind:this={bodyEl}>
            {#each lines as line (line.key)}
                <AskCmdrToolLine {line} {revealPath} />
            {/each}
        </div>
    {/if}
</div>

<style>
    .tool-group {
        display: flex;
        flex-direction: column;
    }

    .group-toggle {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        min-height: 28px;
        padding: var(--spacing-xxs) var(--spacing-xs);
        border: none;
        border-radius: var(--radius-xs);
        background: none;
        color: var(--color-text-secondary);
        font: inherit;
        font-size: var(--font-size-xs);
        text-align: left;
    }

    .group-toggle:not(:disabled):hover {
        background: var(--color-bg-tertiary);
    }

    .chevron {
        display: flex;
        width: 16px;
        justify-content: center;
        flex: none;
        color: var(--color-text-tertiary);
    }

    .summary {
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    /* The lines sit under the header, indented past its chevron so the group reads as one. */
    .group-body {
        display: flex;
        flex-direction: column;
        padding-left: var(--spacing-md);
    }
</style>
