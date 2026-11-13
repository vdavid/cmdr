<!--
  A list of places on the card (a folder's biggest children, its largest files, an app's Library folders): each one a
  button that closes the card and shows that place in the pane (`onGoTo`), with its size at the end of the line.

  ❗ Names come from the item's volume: text nodes, cut with an ellipsis and a tooltip. A size that may be larger says
  "at least"; one the index doesn't have says so, ❌ never 0.
-->
<script lang="ts">
    import type { Snippet } from 'svelte'
    import type { Fact, RecursiveSize } from '#lib/ipc/bindings.js'
    import { t, tString } from '#lib/intl/messages.svelte.js'
    import Trans from '#lib/intl/Trans.svelte'
    import Size from '#lib/ui/Size.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import type { AboutGoTarget } from './about-card'

    export interface PlaceItem {
        name: string
        path: string
        isFolder: boolean
        size: Fact<RecursiveSize>
        /** A muted word after the name: where it sits. `null`: nothing. */
        detail: string | null
    }

    interface Props {
        items: PlaceItem[]
        onGoTo: (target: AboutGoTarget) => void
    }

    const { items, onGoTo }: Props = $props()
</script>

<ul class="places">
    {#each items as item (item.path)}
        <li>
            <button
                type="button"
                class="place"
                use:tooltip={tString('aboutThis.contents.goTo', { name: item.name })}
                onclick={() => {
                    onGoTo({ path: item.path, isFolder: item.isFolder })
                }}
            >
                <span class="glyph" aria-hidden="true"><Icon name={item.isFolder ? 'folder' : 'file'} size={12} /></span>
                <span class="name">{item.name}</span>
                {#if item.detail}<span class="detail">{item.detail}</span>{/if}
                <span class="size">
                    {#if item.size.state === 'known'}
                        {@const bytes = item.size.value.logical}
                        {#if item.size.value.coverage === 'lowerBound'}
                            {#snippet atLeast(_content: Snippet)}<Size {bytes} />{/snippet}
                            <Trans key="aboutThis.contents.atLeastSize" snippets={{ size: atLeast }} />
                        {:else}
                            <Size {bytes} />
                        {/if}
                    {:else}
                        <span class="quiet">{t('aboutThis.contents.noSize')}</span>
                    {/if}
                </span>
            </button>
        </li>
    {/each}
</ul>

<style>
    .places {
        margin: 0;
        padding: 0;
        list-style: none;
        display: flex;
        flex-direction: column;
    }

    .place {
        display: flex;
        gap: var(--spacing-xs);
        align-items: baseline;
        width: 100%;
        padding: 0 var(--spacing-xxs);
        margin: 0 calc(-1 * var(--spacing-xxs));
        border: none;
        border-radius: var(--radius-xs);
        background: none;
        color: inherit;
        font: inherit;
        text-align: start;
        cursor: default;
    }

    .place:hover {
        background: var(--color-bg-tertiary);
    }

    .place:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
        box-shadow: var(--shadow-focus-contrast);
    }

    .glyph {
        flex: none;
        display: inline-flex;
        color: var(--color-text-tertiary);
        position: relative;
        top: 1px; /* An optical nudge onto the text baseline. */
    }

    /* The name keeps its room and the muted "in …" gives way first: it's the part a tooltip can carry. */
    .name {
        flex: 0 1 auto;
        max-width: 65%;
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .detail {
        flex: 1 1 0;
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        color: var(--color-text-tertiary);
    }

    .size {
        flex: none;
        margin-inline-start: auto;
        padding-inline-start: var(--spacing-sm);
        color: var(--color-text-secondary);
        font-variant-numeric: tabular-nums;
    }

    .quiet {
        color: var(--color-text-tertiary);
    }
</style>
