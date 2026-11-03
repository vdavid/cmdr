<!--
    The rail's in-chat find bar (⌘F while the rail has focus). Mounted by `AskCmdrRail`
    above the thread whenever the chat shows, so its keydown listener on the rail is live;
    the bar itself renders only while open. The find's state and keys:
    `ask-cmdr-find.svelte.ts`. The bar: the house `FindBar`.
-->
<script lang="ts">
    import { onDestroy } from 'svelte'
    import FindBar from '#lib/ui/FindBar.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { createRailFind } from './ask-cmdr-find.svelte'

    interface Props {
        /** The rail's root: where the find's keys are heard, and focus's fallback on close. */
        rail: HTMLElement | null
        /** The scrolling thread the find searches. */
        thread: HTMLElement | null
        /** The thread has older messages not loaded yet, so the find covers only part of it. */
        partial: boolean
    }

    const { rail, thread, partial }: Props = $props()

    const find = createRailFind({ getRail: () => rail, getThread: () => thread })

    $effect(() => {
        const el = rail
        if (!el) return
        el.addEventListener('keydown', find.handleKeydown)
        return () => {
            el.removeEventListener('keydown', find.handleKeydown)
        }
    })

    $effect(() => {
        const el = thread
        if (!el || !find.open) return
        return find.observe(el)
    })

    onDestroy(() => {
        find.destroy()
    })

    let barElement = $state<HTMLElement | null>(null)
    $effect(() => {
        find.setBarElement(barElement)
    })
</script>

{#if find.open}
    <div bind:this={barElement}>
        <FindBar
            bind:query={find.query}
            bind:inputElement={find.inputElement}
            regionLabel={tString('askCmdr.find.region')}
            ariaLabel={tString('askCmdr.find.ariaLabel')}
            placeholder={tString('askCmdr.find.placeholder')}
            total={find.total}
            current={find.position}
            note={partial ? tString('askCmdr.find.loadedOnly') : undefined}
            onPrevious={find.previous}
            onNext={find.next}
            onClose={find.closeFind}
        />
    </div>
{/if}

<style>
    /* The CSS Custom Highlight API paints matches without touching the thread's nodes
       (`#lib/ui/find-in-dom.ts`). The current one wears the active colour, as in the viewer. */
    :global(::highlight(cmdr-find-match)) {
        background-color: var(--color-highlight);
    }

    :global(::highlight(cmdr-find-current)) {
        background-color: var(--color-highlight-active);
    }

    /* Older WebKit has no highlight API: the current match's region is outlined instead. */
    :global(.ask-cmdr-rail [data-find-current]) {
        outline: 2px solid var(--color-highlight-active);
        outline-offset: 2px;
        border-radius: var(--radius-sm);
    }
</style>
