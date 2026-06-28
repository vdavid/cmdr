<script lang="ts">
    import { onMount } from 'svelte'
    import { openExternalUrl } from '$lib/tauri-commands'
    import IconsSection from './sections/IconsSection.svelte'
    import SpinnersSection from './sections/SpinnersSection.svelte'
    import StatusBadgesSection from './sections/StatusBadgesSection.svelte'
    import IllustrationsSection from './sections/IllustrationsSection.svelte'
    import AnimationsSection from './sections/AnimationsSection.svelte'
    import IndexingStatusSection from './sections/IndexingStatusSection.svelte'

    interface Props {
        /** Optional sub-anchor (e.g. `'icons'`). Catalog scrolls to `#graphics-<anchor>` when this changes. */
        targetAnchor?: string | null
        /** Fires when a new section scrolls into view. `null` when scrolled to top. */
        onSectionInView?: (subId: string | null) => void
    }

    // This file is both a standalone dev route AND imported as a regular component
    // by the Debug window's sidebar nesting. The page-props lint rule fires on the
    // route side; the component-import use is what gives these props meaning.
    // eslint-disable-next-line svelte/valid-prop-names-in-kit-pages
    const { targetAnchor = null, onSectionInView }: Props = $props()

    /** Ordered sub-ids matching the sidebar order. Used for the IntersectionObserver wiring. */
    const SUB_IDS = ['icons', 'spinners', 'status-badges', 'illustrations', 'animations', 'drive-indexing'] as const

    let rootEl: HTMLElement | undefined = $state()
    let lastScrolledTo: string | null = null
    let observer: IntersectionObserver | undefined
    let suppressObserverUntil = 0

    /** Walk up to the nearest scrollable ancestor (for IntersectionObserver `root`). */
    function findScrollParent(el: HTMLElement | null): HTMLElement | null {
        let current = el?.parentElement ?? null
        while (current) {
            const style = window.getComputedStyle(current)
            if (/(auto|scroll)/.test(style.overflowY)) return current
            current = current.parentElement
        }
        return null
    }

    function scrollToAnchor(subId: string) {
        const el = document.getElementById(`graphics-${subId}`)
        if (!el) return
        suppressObserverUntil = Date.now() + 400
        el.scrollIntoView({ block: 'start', behavior: 'auto' })
    }

    $effect(() => {
        const next = targetAnchor
        if (next === lastScrolledTo) return
        lastScrolledTo = next
        if (next === null) {
            // Parent ("Graphics") clicked: scroll to top.
            const scrollParent = findScrollParent(rootEl ?? null)
            suppressObserverUntil = Date.now() + 400
            scrollParent?.scrollTo({ top: 0, behavior: 'auto' })
        } else {
            scrollToAnchor(next)
        }
    })

    onMount(() => {
        if (!import.meta.env.DEV) return
        const root = findScrollParent(rootEl ?? null)
        observer = new IntersectionObserver(
            (entries) => {
                if (Date.now() < suppressObserverUntil) return
                // Pick the entry closest to the top of the root.
                const visible = entries.filter((e) => e.isIntersecting)
                if (visible.length === 0) return
                visible.sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)
                const first = visible[0]
                const id = first.target.id.replace(/^graphics-/, '')
                if (id !== lastScrolledTo) {
                    lastScrolledTo = id
                    onSectionInView?.(id)
                }
            },
            { root, rootMargin: '0px 0px -60% 0px', threshold: 0 },
        )
        for (const subId of SUB_IDS) {
            const el = document.getElementById(`graphics-${subId}`)
            if (el) observer.observe(el)
        }
        // If a targetAnchor was set on mount, scroll there now (effect already
        // ran but the elements may not have existed yet).
        if (targetAnchor !== null) scrollToAnchor(targetAnchor)
        return () => observer?.disconnect()
    })

    function browserUrl(): string {
        if (typeof window === 'undefined') return ''
        return `${window.location.origin}/dev/graphics`
    }

    async function openInBrowser(event: MouseEvent) {
        event.preventDefault()
        try {
            await openExternalUrl(browserUrl())
        } catch (error) {
            // eslint-disable-next-line no-console -- dev-only catalog; surface failure to console when outside Tauri
            console.warn('Catalog: openExternalUrl failed (likely outside Tauri):', error)
        }
    }
</script>

{#if import.meta.env.DEV}
    <div bind:this={rootEl} class="catalog">
        <header class="catalog-header">
            <h2>Graphics</h2>
            <p>
                Every visual asset the app renders: icons, spinners, status badges, illustrations, and animations. Each
                item carries a tooltip describing where it shows up in the app, so a designer can review them for
                consistency.
            </p>
            <p class="catalog-browser-link">
                <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- href is decorative; onclick routes through openExternalUrl -->
                <a href={browserUrl()} onclick={openInBrowser}>Open in browser ↗</a>
            </p>
        </header>

        <IconsSection />
        <SpinnersSection />
        <StatusBadgesSection />
        <IllustrationsSection />
        <AnimationsSection />
        <IndexingStatusSection />
    </div>
{/if}

<style>
    .catalog {
        display: flex;
        flex-direction: column;
    }

    .catalog-header {
        margin-bottom: var(--spacing-xl);
    }

    .catalog-header h2 {
        margin: 0 0 var(--spacing-xs);
        font-size: var(--font-size-lg);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .catalog-header p {
        margin: 0 0 var(--spacing-xs);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .catalog-browser-link a {
        font-size: var(--font-size-sm);
        color: var(--color-accent-text);
        text-decoration: underline;
    }
</style>
