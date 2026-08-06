<script lang="ts">
    /**
     * Post-update "What's new" popup: renders the changelog slice for the releases between
     * the version the user last saw and the one running now. Driven by the reactive
     * `whatsNewState`; mounted from `routes/(main)/+page.svelte` while `whatsNewState.open`
     * is true.
     *
     * Each release shows only its highlights (the lead) up front; the Added / Changed /
     * Fixed / Security lists sit behind a per-release "Show more" disclosure, collapsed by
     * default, so a multi-release slice reads as a short list of headlines instead of a wall
     * of entries.
     *
     * The auto-popup never opens empty (the trigger collapses an empty slice to a silent
     * stamp); the empty state is reachable only via the manual Help reopen.
     */
    import snarkdown from 'snarkdown'
    import { SvelteSet } from 'svelte/reactivity'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import { addToast } from '$lib/ui/toast'
    import { openExternalUrl } from '$lib/tauri-commands'
    import { setSetting } from '$lib/settings'
    import { getAppLogger } from '$lib/logging/logger'
    import { whatsNewState, closeWhatsNew } from './whats-new-trigger.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    const log = getAppLogger('whatsNewDialog')

    const CHANGELOG_URL = 'https://getcmdr.com/changelog/'

    const releases = $derived(whatsNewState.releases)
    const isEmpty = $derived(releases.length === 0)

    /**
     * The versions whose detail lists are open. A `SvelteSet` (not a keyed object) so
     * `has()` always answers a real boolean: an absent key would make `aria-expanded`
     * `undefined`, which drops the attribute off the disclosure button. The dialog mounts
     * fresh on every open, so everything starts collapsed each time.
     */
    const expandedVersions = new SvelteSet<string>()

    function detailsId(version: string): string {
        return `whats-new-details-${version}`
    }

    function toggleDetails(version: string) {
        if (expandedVersions.has(version)) {
            expandedVersions.delete(version)
        } else {
            expandedVersions.add(version)
        }
    }

    /**
     * Renders trusted changelog markdown to HTML. `{@html}` is safe here: the content is
     * our own committed `CHANGELOG.md` (parsed backend-side), not user input. Same trust
     * level as `FriendlyError`'s `md!` output that `renderErrorMarkdown` renders.
     */
    function renderMarkdown(md: string): string {
        return snarkdown(md)
    }

    async function handleOpenChangelog() {
        try {
            await openExternalUrl(CHANGELOG_URL)
        } catch (e) {
            log.warn("Couldn't open the changelog link: {error}", { error: String(e) })
        }
    }

    function handleClose() {
        closeWhatsNew()
    }

    function handleOptOut() {
        setSetting('whatsNew.showOnUpdate', false)
        closeWhatsNew()
        addToast(tString('whatsNew.optOutToast'), {
            level: 'default',
        })
    }
</script>

<ModalDialog
    titleId="whats-new-title"
    dialogId="whats-new"
    role="dialog"
    onclose={handleClose}
    ariaDescribedby="whats-new-body"
    fillBody
    containerStyle="width: 560px; max-width: calc(100vw - 2 * var(--spacing-xl)); max-height: calc(0.9 * (100vh - var(--titlebar-height)))"
>
    {#snippet title()}{tString('whatsNew.dialog.title')}{/snippet}

    <div class="body" id="whats-new-body">
        <div class="scroll-area">
            {#if isEmpty}
                <p class="empty">{tString('whatsNew.dialog.empty')}</p>
            {:else}
                {#each releases as release (release.version)}
                    <section class="release">
                        <h3 class="release-heading">
                            <span class="version">{release.version}</span>
                            <span class="dot" aria-hidden="true">·</span>
                            <span class="date">{release.date}</span>
                        </h3>
                        {#if release.lead != null}
                            <!-- A <div>, not a <p>: a lead can be a bold headline plus a Markdown numbered
                                 list, and snarkdown emits a block <ol> that's invalid inside a <p>. -->
                            <!-- eslint-disable-next-line svelte/no-at-html-tags -- trusted: renders our committed CHANGELOG via renderMarkdown(), not user input -->
                            <div class="lead">{@html renderMarkdown(release.lead)}</div>
                        {/if}
                        {#if release.sections.length > 0}
                            <button
                                class="details-toggle"
                                onclick={() => { toggleDetails(release.version); }}
                                aria-expanded={expandedVersions.has(release.version)}
                                aria-controls={detailsId(release.version)}
                            >
                                <!-- A text triangle, not an <Icon>: the marker column holds a
                                     typographic glyph (the bullet) in every other row, and only
                                     another glyph lines up with it. An SVG icon carries its own
                                     transparent padding, which reads as a misaligned chevron.
                                     Swapping the glyph rather than rotating one keeps the marker
                                     planted: a rotated glyph pivots around its box, not its ink. -->
                                <span class="toggle-marker" aria-hidden="true"
                                    >{expandedVersions.has(release.version) ? '▾' : '▸'}</span
                                >
                                {expandedVersions.has(release.version)
                                    ? tString('whatsNew.dialog.showLess')
                                    : tString('whatsNew.dialog.showMore')}
                            </button>
                            <!-- Height-animated disclosure: the 0fr→1fr grid row is what transitions.
                                 `inert` while collapsed keeps the hidden entries out of the tab order
                                 and the a11y tree without taking them out of the layout the animation
                                 measures. -->
                            <div
                                class="details"
                                class:expanded={expandedVersions.has(release.version)}
                                id={detailsId(release.version)}
                                inert={!expandedVersions.has(release.version)}
                            >
                                <div class="details-inner">
                                    <SectionCard>
                                        {#each release.sections as section (section.title)}
                                            <h4 class="section-title">{section.title}</h4>
                                            <ul class="entries">
                                                {#each section.entries as entry, i (i)}
                                                    <!-- eslint-disable-next-line svelte/no-at-html-tags -- trusted: renders our committed CHANGELOG via renderMarkdown(), not user input -->
                                                    <li>{@html renderMarkdown(entry)}</li>
                                                {/each}
                                            </ul>
                                        {/each}
                                    </SectionCard>
                                </div>
                            </div>
                        {/if}
                    </section>
                {/each}
            {/if}

        </div>

        <!-- Outside the scroll area: the way to the full changelog stays in view however long
             the slice is. -->
        <p class="full-changelog">
            <LinkButton
                href={CHANGELOG_URL}
                onclick={(e: MouseEvent) => {
                    e.preventDefault()
                    void handleOpenChangelog()
                }}>{tString('whatsNew.dialog.seeFullChangelog')}</LinkButton
            >
        </p>

        <div class="footer">
            <Button variant="secondary" onclick={handleOptOut}>{tString('whatsNew.dialog.optOut')}</Button>
            <Button variant="primary" onclick={handleClose}>{tString('whatsNew.dialog.close')}</Button>
        </div>
    </div>
</ModalDialog>

<style>
    /* `fillBody` makes the panel a flex column capped at 90% of the window (see
       `containerStyle`): this region takes the slack, `.scroll-area` inside it scrolls, and a
       collapsed slice still shrink-wraps to a short dialog. */
    .body {
        /* The marker column: bullets need barely more than the glyph, a numbered lead needs
           room for "10.". Both keep wrapped lines aligned with the first line's text. */
        --spacing-whats-new-marker: 1.15em;

        display: flex;
        flex-direction: column;
        flex: 1 1 auto;
        min-height: 0;
    }

    .scroll-area {
        overflow-y: auto;
        min-height: 0;
        /* Keep a little room so the scrollbar doesn't crowd the text. */
        padding-right: var(--spacing-xs);
    }

    .empty {
        margin: var(--spacing-md) 0;
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
    }

    .release {
        margin-bottom: var(--spacing-xl);
    }

    .release:last-of-type {
        margin-bottom: var(--spacing-md);
    }

    .release-heading {
        display: flex;
        align-items: baseline;
        gap: var(--spacing-sm);
        margin: 0 0 var(--spacing-sm);
    }

    /* Heading and body set the same size: color carries the hierarchy, not scale. */
    .version {
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .dot,
    .date {
        font-size: var(--font-size-md);
        font-weight: 400;
        color: var(--color-text-tertiary);
    }

    .lead {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: var(--font-line-height-prose);
    }

    /* The lead's bold headline (the part most people read): lift it to the primary
       text color so it reads as the summary, above the secondary-toned detail. */
    .lead :global(strong) {
        color: var(--color-text-primary);
        font-weight: 600;
    }

    /* Lists sit flush with the surrounding text: the marker occupies its own column, so a
       wrapped line lines up under the first line instead of under the bullet. Applies to a
       lead's authored list (snarkdown emits a bare <ul> / <ol>) and the entry lists alike. */
    .lead :global(ul),
    .lead :global(ol),
    .entries {
        /* Top margin only: the block below owns its own leading, so a list can't stack two
           gaps at the end of a section. */
        margin: var(--spacing-md) 0 0;
        padding: 0;
        list-style: none;
    }

    .lead :global(li),
    .entries li {
        display: grid;
        grid-template-columns: var(--spacing-whats-new-marker) 1fr;
        /* Baseline, not stretch: the oversized marker glyph sits on the text's first-line
           baseline instead of pushing the row taller. */
        align-items: baseline;
    }

    .lead :global(li + li),
    .entries li + li {
        margin-top: var(--spacing-xs);
    }

    .lead :global(li)::before,
    .entries li::before {
        color: var(--color-text-tertiary);
    }

    .lead :global(ul) > :global(li)::before,
    .entries li::before {
        content: '•';
    }

    /* A numbered lead: the counter replaces the list marker, in a column wide enough for "10.". */
    .lead :global(ol) {
        counter-reset: lead-item;
    }

    .lead :global(ol) > :global(li) {
        grid-template-columns: 1.6em 1fr;
    }

    .lead :global(ol) > :global(li)::before {
        counter-increment: lead-item;
        content: counter(lead-item) '.';
    }

    /* Same two-column grid as a list item, at the same font size: the chevron lands where a
       bullet would, and the label where the entry text does. */
    /* Same two-column grid as a list item, at the same font size: the marker lands where a
       bullet would, the label where the entry text does. Full width so the whole row is the
       hit area. */
    .details-toggle {
        display: grid;
        grid-template-columns: var(--spacing-whats-new-marker) 1fr;
        align-items: baseline;
        width: 100%;
        padding: var(--spacing-xs) 0;
        background: none;
        border: none;
        font-size: var(--font-size-md);
        text-align: left;
        /* The AA-safe accent every interactive text in the app uses (see `LinkButton`). */
        color: var(--color-accent-text);
    }

    .details-toggle:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
        border-radius: var(--radius-sm);
    }

    /* Bullets and the disclosure triangle are oversized on purpose: at text size a "•" or a
       "▸" reads as a speck. The `1` line height keeps the bigger glyph from stretching its
       row. Numbered markers stay at text size, where digits belong. */
    .toggle-marker,
    .lead :global(ul) > :global(li)::before,
    .entries li::before {
        font-size: 1.35em;
        line-height: var(--font-line-height-flat);
    }

    /* The 0fr → 1fr row is the animated height; the inner element clips while it grows. */
    .details {
        display: grid;
        grid-template-rows: 0fr;
    }

    .details.expanded {
        grid-template-rows: 1fr;
    }

    @media (prefers-reduced-motion: no-preference) {
        .details {
            transition: grid-template-rows var(--transition-slow);
        }
    }

    .details-inner {
        overflow: hidden;
        min-height: 0;
    }

    /* A margin, never padding: the inner box is clipped to zero height while collapsed, so a
       margin disappears with it while padding would leave a visible gap. */
    .details-inner :global(.section-card-wrap) {
        margin-top: var(--spacing-xs);
    }

    .section-title {
        margin: var(--spacing-md) 0 var(--spacing-xs);
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
        line-height: var(--font-line-height-prose);
    }

    .section-title:first-child {
        margin-top: 0;
    }

    /* Same size and rhythm as the lead: the entries are the same kind of prose, only
       secondary in importance, which the color already says. */
    .entries li {
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: var(--font-line-height-prose);
    }

    /* Inline markdown from the changelog: keep code/quotes readable inside list items. */
    .entries li :global(code),
    .lead :global(code) {
        font-family: var(--font-mono);
        font-size: 0.92em;
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-sm);
        padding: 0 var(--spacing-xxs);
    }

    .full-changelog {
        margin: var(--spacing-md) 0 0;
        font-size: var(--font-size-md);
    }

    .footer {
        display: flex;
        align-items: center;
        justify-content: flex-end;
        gap: var(--spacing-md);
        margin-top: var(--spacing-lg);
    }
</style>
