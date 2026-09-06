<script lang="ts">
    /**
     * Shared frame for the Debug window's catalog pages (Components, Graphics): the
     * header with its "Open in browser" link, plus the sidebar sync that scrolls to a
     * requested section and reports which section is in view.
     *
     * Every section carries `id="<prefix>-<subId>"`; the page passes `subIds` in
     * sidebar order.
     */
    import { onMount, type Snippet } from 'svelte'
    import { openExternalUrl } from '$lib/tauri-commands'

    interface Props {
        /** Anchor id prefix (`'components'`): every section is `#<prefix>-<subId>`. */
        prefix: string
        /** Ordered sub-ids matching the sidebar order. Used for the IntersectionObserver wiring. */
        subIds: readonly string[]
        /** Standalone route path (`'/dev/components'`) behind the "Open in browser" link. */
        route: string
        title: string
        /** Optional sub-anchor (e.g. `'buttons'`). Catalog scrolls to `#<prefix>-<anchor>` when this changes. */
        targetAnchor?: string | null
        /** Fires when a new section scrolls into view. `null` when scrolled to top. */
        onSectionInView?: (subId: string | null) => void
        description: Snippet
        children: Snippet
    }

    const {
        prefix,
        subIds,
        route,
        title,
        targetAnchor = null,
        onSectionInView,
        description,
        children,
    }: Props = $props()

    let rootEl: HTMLElement | undefined = $state()
    let lastScrolledTo: string | null = null
    let observer: IntersectionObserver | undefined
    let suppressObserverUntil = 0

    function anchorId(subId: string): string {
        return `${prefix}-${subId}`
    }

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
        const el = document.getElementById(anchorId(subId))
        if (!el) return
        suppressObserverUntil = Date.now() + 400
        el.scrollIntoView({ block: 'start', behavior: 'auto' })
    }

    $effect(() => {
        const next = targetAnchor
        if (next === lastScrolledTo) return
        lastScrolledTo = next
        if (next === null) {
            // Parent (the catalog's own sidebar entry) clicked: scroll to top.
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
                // Observed elements all carry the `<prefix>-` anchor prefix.
                const id = first.target.id.slice(prefix.length + 1)
                if (id !== lastScrolledTo) {
                    lastScrolledTo = id
                    onSectionInView?.(id)
                }
            },
            { root, rootMargin: '0px 0px -60% 0px', threshold: 0 },
        )
        for (const subId of subIds) {
            const el = document.getElementById(anchorId(subId))
            if (el) observer.observe(el)
        }
        // If a targetAnchor was set on mount, scroll there now (effect already
        // ran but the elements may not have existed yet).
        if (targetAnchor !== null) scrollToAnchor(targetAnchor)
        return () => observer?.disconnect()
    })

    function browserUrl(): string {
        if (typeof window === 'undefined') return ''
        return `${window.location.origin}${route}`
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

<div bind:this={rootEl} class="catalog">
    <header class="catalog-header">
        <h2>{title}</h2>
        <p>{@render description()}</p>
        <p class="catalog-browser-link">
            <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- href is decorative; onclick routes through openExternalUrl -->
            <a href={browserUrl()} onclick={openInBrowser}>Open in browser ↗</a>
        </p>
    </header>

    {@render children()}
</div>

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
