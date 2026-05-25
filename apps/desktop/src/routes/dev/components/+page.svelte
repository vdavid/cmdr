<script lang="ts">
    import { onMount } from 'svelte'
    import { openExternalUrl } from '$lib/tauri-commands'
    import Buttons from './sections/Buttons.svelte'
    import Links from './sections/Links.svelte'
    import Groups from './sections/Groups.svelte'
    import Dialogs from './sections/Dialogs.svelte'
    import Toasts from './sections/Toasts.svelte'
    import Progress from './sections/Progress.svelte'
    import Loading from './sections/Loading.svelte'
    import Tooltips from './sections/Tooltips.svelte'
    import SizeBadges from './sections/SizeBadges.svelte'
    import CommandBoxSection from './sections/CommandBoxSection.svelte'
    import EmptyStates from './sections/EmptyStates.svelte'

    interface Props {
        /** Optional sub-anchor (e.g. `'buttons'`). Catalog scrolls to `#components-<anchor>` when this changes. */
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
    const SUB_IDS = [
        'buttons',
        'links',
        'groups',
        'dialogs',
        'toasts',
        'progress',
        'loading',
        'tooltips',
        'size-badges',
        'commandbox',
        'empty-states',
    ] as const

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
        const el = document.getElementById(`components-${subId}`)
        if (!el) return
        suppressObserverUntil = Date.now() + 400
        el.scrollIntoView({ block: 'start', behavior: 'auto' })
    }

    $effect(() => {
        const next = targetAnchor
        if (next === lastScrolledTo) return
        lastScrolledTo = next
        if (next === null) {
            // Parent ("Components") clicked: scroll to top.
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
                const id = first.target.id.replace(/^components-/, '')
                if (id !== lastScrolledTo) {
                    lastScrolledTo = id
                    onSectionInView?.(id)
                }
            },
            { root, rootMargin: '0px 0px -60% 0px', threshold: 0 },
        )
        for (const subId of SUB_IDS) {
            const el = document.getElementById(`components-${subId}`)
            if (el) observer.observe(el)
        }
        // If a targetAnchor was set on mount, scroll there now (effect already
        // ran but the elements may not have existed yet).
        if (targetAnchor !== null) scrollToAnchor(targetAnchor)
        return () => observer?.disconnect()
    })

    function browserUrl(): string {
        if (typeof window === 'undefined') return ''
        return `${window.location.origin}/dev/components`
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
            <h2>Components</h2>
            <p>
                Catalog of every primitive in <code>lib/ui</code>. Add new primitives here — see
                <code>lib/ui/CLAUDE.md</code>.
            </p>
            <p class="catalog-browser-link">
                <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- href is decorative; onclick routes through openExternalUrl -->
                <a href={browserUrl()} onclick={openInBrowser}>Open in browser ↗</a>
            </p>
        </header>

        <Buttons />
        <Links />
        <Groups />
        <Dialogs />
        <Toasts />
        <Progress />
        <Loading />
        <Tooltips />
        <SizeBadges />
        <CommandBoxSection />
        <EmptyStates />
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

    .catalog-header code {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        background: var(--color-bg-tertiary);
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- Inline-code pill: 1px 4px is tighter than any spacing token */
        padding: 1px 4px;
        border-radius: var(--radius-sm);
    }

    .catalog-browser-link a {
        font-size: var(--font-size-sm);
        color: var(--color-accent-text);
        text-decoration: underline;
    }
</style>
