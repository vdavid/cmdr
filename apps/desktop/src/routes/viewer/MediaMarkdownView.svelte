<script lang="ts">
    /**
     * Formatted view for the viewer's `markdown` kind. The backend reads, renders, and
     * sanitizes the document (`viewerRenderMarkdown`); this shows the HTML and wires its
     * images (`markdown-view.ts`). Text shows as soon as the render lands; each picture
     * fills in as its `cmdr-media://` fetch completes, a phone's included.
     *
     * Web images stay held back until the user presses "Load remote images" (loading
     * them would tell each site the user's IP address, like an email tracking pixel).
     * Links never navigate the viewer's webview: a click hands its `href` to the page
     * (`onFollowLink`), which asks the backend where it goes.
     */
    import { tick } from 'svelte'
    import Button from '#lib/ui/Button.svelte'
    import Spinner from '#lib/ui/Spinner.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { viewerAllowRemoteImages, viewerRenderMarkdown } from '#lib/tauri-commands/index.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import {
        clickedLinkHref,
        findAnchorTarget,
        loadRemoteImages,
        wireImages,
        type MarkdownLanding,
        type PlaceholderLabels,
    } from './markdown-view'
    import { focusQuietly } from './quiet-focus'

    const log = getAppLogger('viewer')

    type Props = {
        sessionId: string
        /** File name, the document region's accessible name. */
        fileName: string
        /** Where the document opens once rendered; read once per render. `null` is the top. */
        landing: MarkdownLanding | null
        /** A link was clicked: the page resolves and follows it. */
        onFollowLink: (href: string) => void
    }

    const { sessionId, fileName, landing, onFollowLink }: Props = $props()

    type LoadState = 'loading' | 'ready' | 'error'
    let loadState = $state<LoadState>('loading')
    let html = $state('')
    let hasRemoteImages = $state(false)
    let remoteImagesLoaded = $state(false)
    let scrollRef: HTMLDivElement | undefined = $state()
    let articleRef: HTMLElement | undefined = $state()

    function placeholderLabels(): PlaceholderLabels {
        return {
            remote: tString('viewer.markdown.image.remote'),
            blocked: tString('viewer.markdown.image.blocked'),
            missing: tString('viewer.markdown.image.missing'),
        }
    }

    $effect(() => {
        const id = sessionId
        loadState = 'loading'
        remoteImagesLoaded = false
        viewerRenderMarkdown(id)
            .then(async (rendered) => {
                if (id !== sessionId) return
                html = rendered.html
                hasRemoteImages = rendered.hasRemoteImages
                loadState = 'ready'
                await tick()
                if (articleRef) wireImages(articleRef, placeholderLabels())
                land(landing)
                if (scrollRef) focusQuietly(scrollRef)
            })
            .catch((e: unknown) => {
                if (id !== sessionId) return
                log.warn('Formatting the Markdown document didn’t work: {error}', { error: e })
                loadState = 'error'
            })
    })

    async function loadRemote(): Promise<void> {
        try {
            await viewerAllowRemoteImages(sessionId)
        } catch (e) {
            log.warn('Allowing remote images didn’t work: {error}', { error: e })
            return
        }
        remoteImagesLoaded = true
        if (articleRef) loadRemoteImages(articleRef, placeholderLabels())
    }

    /**
     * Opens the document where `target` says. A scroll position restores as far as the
     * document reaches now: pictures still loading may shift it a little.
     */
    function land(target: MarkdownLanding | null): void {
        if (target?.kind === 'scroll') {
            if (scrollRef) scrollRef.scrollTop = target.top
        } else {
            scrollToElement(target?.elementId ?? null)
        }
    }

    /** A click on a link never navigates the webview: the page decides where it goes. */
    function handleClick(e: MouseEvent): void {
        if (!(e.target instanceof Element) || !e.target.closest('a')) return
        e.preventDefault()
        const href = clickedLinkHref(e.target)
        if (href) onFollowLink(href)
    }

    /** Scrolls to the element an in-page link names, or to the top for `null` or a missing one. */
    export function scrollToElement(elementId: string | null): void {
        const target = elementId && articleRef ? findAnchorTarget(articleRef, elementId) : null
        if (target) target.scrollIntoView({ block: 'start' })
        else if (scrollRef) scrollRef.scrollTop = 0
    }

    /** How far the document is scrolled, so Back can bring the user back to it. */
    export function getScrollTop(): number {
        return scrollRef?.scrollTop ?? 0
    }

    /** Edit > Select all: the document's text, not the window's chrome. */
    export function selectAll(): void {
        if (articleRef) window.getSelection()?.selectAllChildren(articleRef)
    }
</script>

<div class="markdown-stage">
    {#if loadState === 'loading'}
        <div class="markdown-status" role="status">
            <Spinner size="lg" />
            <span class="sr-only">{tString('viewer.markdown.loading')}</span>
        </div>
    {:else if loadState === 'error'}
        <div class="markdown-status markdown-error" role="alert">
            <p>{tString('viewer.markdown.error')}</p>
        </div>
    {:else}
        {#if hasRemoteImages && !remoteImagesLoaded}
            <div class="remote-bar">
                <span>{tString('viewer.markdown.remoteImages.notice')}</span>
                <Button size="mini" variant="secondary" onclick={() => void loadRemote()}>
                    {tString('viewer.markdown.remoteImages.load')}
                </Button>
            </div>
        {/if}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
        <div class="markdown-scroll" bind:this={scrollRef} tabindex="0"
            role="document"
            aria-label={fileName}
            onclick={handleClick}
        >
            <!-- eslint-disable-next-line svelte/no-at-html-tags -- sanitized by the backend's ammonia allowlist (`markdown_render.rs`): no script, handler, form, frame, or style survives -->
            <article class="markdown-body" bind:this={articleRef}>{@html html}</article>
        </div>
    {/if}
</div>

<style>
    .markdown-stage {
        position: relative;
        flex: 1;
        min-height: 0;
        display: flex;
        flex-direction: column;
        background-color: var(--color-bg-primary);
    }

    .markdown-status {
        position: absolute;
        inset: 0;
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-lg);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        text-align: center;
    }

    .markdown-error {
        color: var(--color-error);
    }

    .markdown-error p {
        margin: 0;
        max-width: 28rem;
    }

    .remote-bar {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-md);
        padding: var(--spacing-xs) var(--spacing-lg);
        border-bottom: 1px solid var(--color-border-subtle);
        background-color: var(--color-bg-secondary);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .markdown-scroll {
        flex: 1;
        min-height: 0;
        overflow: auto;
        outline: none;
    }

    .markdown-scroll:focus-visible:not([data-quiet-focus]) {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
    }

    /* The document: a readable measure, centered, in the system font. Everything inside
       arrives as `{@html}`, so every rule below reaches it through `:global`, and the
       classes the renderer emits (`markdown_render.rs`) are matched as attributes
       (`[class~='…']`): no Svelte markup carries them for the unused-class scan to see. */
    .markdown-body {
        max-width: 48rem;
        margin: 0 auto;
        padding: var(--spacing-2xl) var(--spacing-xl);
        color: var(--color-text-primary);
        font-family: var(--font-system);
        font-size: var(--font-size-lg);
        line-height: var(--font-line-height-prose);
        overflow-wrap: break-word;
        user-select: text;
        -webkit-user-select: text;
    }

    .markdown-body > :global(:first-child) {
        margin-top: 0;
    }

    .markdown-body > :global(:last-child) {
        margin-bottom: 0;
    }

    .markdown-body :global(p),
    .markdown-body :global(ul),
    .markdown-body :global(ol),
    .markdown-body :global(dl),
    .markdown-body :global(blockquote),
    .markdown-body :global(pre),
    .markdown-body :global(table),
    .markdown-body :global(details) {
        margin: 0 0 1em;
    }

    .markdown-body :global(:is(h1, h2, h3, h4, h5, h6)) {
        margin: 1.5em 0 0.6em;
        font-weight: 600;
        line-height: var(--font-line-height-tight);
    }

    /* A document's heading ladder, relative to its body text so it follows the user's font
       scale. No token covers it: these are display sizes. */
    .markdown-body :global(h1) {
        padding-bottom: 0.3em;
        border-bottom: 1px solid var(--color-border-subtle);
        font-size: 2em;
    }

    .markdown-body :global(h2) {
        padding-bottom: 0.3em;
        border-bottom: 1px solid var(--color-border-subtle);
        font-size: 1.5em;
    }

    .markdown-body :global(h3) {
        font-size: 1.25em;
    }

    .markdown-body :global(h4) {
        font-size: 1em;
    }

    .markdown-body :global(h5) {
        font-size: 0.875em;
    }

    .markdown-body :global(h6) {
        color: var(--color-text-secondary);
        font-size: 0.85em;
    }

    .markdown-body :global(a) {
        color: var(--color-accent-text);
        text-decoration: underline;
        text-underline-offset: 0.15em;
    }

    .markdown-body :global(ul),
    .markdown-body :global(ol) {
        padding-left: 2em;
    }

    .markdown-body :global(li + li),
    .markdown-body :global(li > ul),
    .markdown-body :global(li > ol) {
        margin-top: 0.25em;
        margin-bottom: 0;
    }

    /* A task list item: the read-only checkbox stands in for the bullet. */
    .markdown-body :global(li:has(> input[type='checkbox'])) {
        list-style: none;
    }

    .markdown-body :global(li > input[type='checkbox']) {
        margin: 0 0.4em 0 -1.4em;
        vertical-align: middle;
    }

    .markdown-body :global(blockquote) {
        padding: 0 1em;
        border-left: 0.25em solid var(--color-border-strong);
        color: var(--color-text-secondary);
    }

    .markdown-body :global(blockquote[class~='markdown-alert-note']) {
        border-left-color: var(--color-info-border);
    }

    .markdown-body :global(blockquote[class~='markdown-alert-tip']) {
        border-left-color: var(--color-allow);
    }

    .markdown-body :global(blockquote[class~='markdown-alert-important']) {
        border-left-color: var(--color-accent);
    }

    .markdown-body :global(blockquote[class~='markdown-alert-warning']) {
        border-left-color: var(--color-warning);
    }

    .markdown-body :global(blockquote[class~='markdown-alert-caution']) {
        border-left-color: var(--color-error);
    }

    .markdown-body :global(:is(code, kbd, samp)) {
        font-family: var(--font-mono);
        font-size: 0.875em;
    }

    .markdown-body :global(:not(pre) > code) {
        padding: 0.15em 0.35em;
        border-radius: var(--radius-sm);
        background-color: var(--color-bg-secondary);
    }

    .markdown-body :global(kbd) {
        padding: 0.1em 0.4em;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background-color: var(--color-bg-secondary);
    }

    .markdown-body :global(pre) {
        padding: var(--spacing-lg);
        overflow: auto;
        border-radius: var(--radius-md);
        background-color: var(--color-bg-secondary);
        line-height: var(--font-line-height-normal);
    }

    .markdown-body :global(pre > code) {
        font-size: 0.85em;
        white-space: pre;
    }

    .markdown-body :global(table) {
        display: block;
        width: max-content;
        max-width: 100%;
        overflow: auto;
        border-collapse: collapse;
    }

    .markdown-body :global(:is(th, td)) {
        padding: 0.4em 0.8em;
        border: 1px solid var(--color-border);
    }

    .markdown-body :global(th) {
        background-color: var(--color-bg-secondary);
        font-weight: 600;
    }

    .markdown-body :global(tbody tr:nth-child(2n)) {
        background-color: var(--color-bg-stripe);
    }

    .markdown-body :global(hr) {
        margin: 1.5em 0;
        border: 0;
        border-top: 1px solid var(--color-border);
    }

    .markdown-body :global(img) {
        max-width: 100%;
        height: auto;
    }

    .markdown-body :global([class~='footnote-definition']) {
        color: var(--color-text-secondary);
        font-size: 0.875em;
    }

    .markdown-body :global([class~='footnote-definition'] > p) {
        display: inline;
    }

    /* Stands in for a picture that isn't shown: held back, refused, or missing. */
    .markdown-body :global([data-md-placeholder]) {
        display: inline-block;
        max-width: 100%;
        padding: 0.15em 0.6em;
        border: 1px dashed var(--color-border-strong);
        border-radius: var(--radius-sm);
        color: var(--color-text-secondary);
        font-size: 0.8em;
    }
</style>
