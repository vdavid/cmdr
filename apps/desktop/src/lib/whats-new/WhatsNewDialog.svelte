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
     * stamp); the empty state is reachable only via the manual Help reopen. A security notice
     * (advisories fixed since the version last seen) sits above the releases, and can be the
     * whole content when changelogs are turned off.
     */
    import { SvelteSet } from 'svelte/reactivity'
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import Button from '#lib/ui/Button.svelte'
    import LinkButton from '#lib/ui/LinkButton.svelte'
    import SectionCard from '#lib/ui/SectionCard.svelte'
    import { addToast } from '#lib/ui/toast/index.js'
    import { openExternalUrl } from '#lib/tauri-commands/index.js'
    import { handleMarkdownLinkClick } from '#lib/ui/markdown-link-click.js'
    import { setSetting } from '#lib/settings/index.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import { whatsNewState, closeWhatsNew } from './whats-new-trigger.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'

    const log = getAppLogger('whatsNewDialog')

    const CHANGELOG_URL = 'https://getcmdr.com/changelog/'

    const releases = $derived(whatsNewState.releases)
    /**
     * Security fixes since the version last seen, most severe first. Links arrive vetted against
     * the advisory allowlist backend-side; one it refused is `null` and still counts.
     */
    const advisories = $derived(whatsNewState.advisories)
    const linkedAdvisories = $derived(
        advisories.flatMap((advisory) => (advisory.url === null ? [] : [{ id: advisory.id, url: advisory.url }])),
    )
    // A notice on its own is content too: the empty state is for a dialog with nothing at all.
    const isEmpty = $derived(releases.length === 0 && advisories.length === 0)

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

    async function handleOpenChangelog() {
        try {
            await openExternalUrl(CHANGELOG_URL)
        } catch (e) {
            log.warn("Couldn't open the changelog link: {error}", { error: e })
        }
    }

    async function handleOpenAdvisory(url: string) {
        try {
            await openExternalUrl(url)
        } catch (e) {
            log.warn("Couldn't open the security advisory link: {error}", { error: e })
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
        {#if advisories.length > 0}
            <!-- Above the scroll area, so it stays in view however long the slice is, and
                 outside its markdown click delegate, so a link here opens exactly once. -->
            <section class="security-notice" aria-labelledby="whats-new-security-notice">
                <p class="security-notice-title" id="whats-new-security-notice">
                    {tString('whatsNew.securityNotice.title', { count: advisories.length })}
                </p>
                {#if linkedAdvisories.length > 0}
                    <ul class="security-notice-links">
                        {#each linkedAdvisories as advisory (advisory.id)}
                            <li>
                                <LinkButton
                                    href={advisory.url}
                                    onclick={(e: MouseEvent) => {
                                        e.preventDefault()
                                        void handleOpenAdvisory(advisory.url)
                                    }}
                                    >{advisories.length === 1
                                        ? tString('whatsNew.securityNotice.readAdvisory')
                                        : tString('whatsNew.securityNotice.readAdvisoryNamed', {
                                              id: advisory.id,
                                          })}</LinkButton
                                >
                            </li>
                        {/each}
                    </ul>
                {/if}
            </section>
        {/if}

        <!-- Click delegate for anchor tags inside rendered markdown: an entry or a lead
             can carry a Markdown link, and Tauri blocks raw <a> navigation. Each link is
             focusable, and its Enter bubbles here as a click. -->
        <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
        <div class="scroll-area" onclick={handleMarkdownLinkClick}>
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
                        {#if release.leadHtml != null}
                            <!-- A <div>, not a <p>: the lead is CommonMark block HTML (paragraphs,
                                 lists), which a <p> can't contain. -->
                            <!-- eslint-disable-next-line svelte/no-at-html-tags -- trusted: our committed CHANGELOG, rendered to HTML by the backend, not user input -->
                            <div class="lead">{@html release.leadHtml}</div>
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
                                                {#each section.entriesHtml as entryHtml, i (i)}
                                                    <!-- eslint-disable-next-line svelte/no-at-html-tags -- trusted: our committed CHANGELOG, rendered to HTML by the backend, not user input -->
                                                    <li>{@html entryHtml}</li>
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
    </div>

    {#snippet footer()}
        <Button variant="secondary" onclick={handleOptOut}>{tString('whatsNew.dialog.optOut')}</Button>
        <Button variant="primary" onclick={handleClose}>{tString('whatsNew.dialog.close')}</Button>
    {/snippet}
</ModalDialog>

<style>
    /* `fillBody` makes the panel a flex column capped at 90% of the window (see
       `containerStyle`): this region takes the slack, `.scroll-area` inside it scrolls, and a
       collapsed slice still shrink-wraps to a short dialog. */
    .body {
        /* The marker column: bullets need barely more than the glyph, a numbered lead needs
           room for "10.". Both keep wrapped lines aligned with the first line's text. */
        --spacing-whats-new-marker: 1.15em;
        /* How much bigger the bullet glyph is than the text it marks. Named rather than
           inlined because the marker's own box has to divide it back out (see below). */
        --font-whats-new-marker-scale: 1.35;

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
        /* A reserved gutter, pulled out into the body's inset so the text keeps its edge
           with a classic scrollbar too (#889). `#lib/ui/DETAILS.md` § "Scrollbar gutters". */
        scrollbar-gutter: stable;
        margin-right: calc(var(--spacing-scrollbar-width) * -1);
    }

    /* Calm on purpose: the fix is already running, so this informs rather than warns. */
    .security-notice {
        flex: none;
        margin: 0 0 var(--spacing-lg);
        padding: var(--spacing-sm) var(--spacing-md);
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-md);
        font-size: var(--font-size-md);
    }

    .security-notice-title {
        margin: 0;
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .security-notice-links {
        margin: var(--spacing-xs) 0 0;
        padding: 0;
        list-style: none;
    }

    .security-notice-links li + li {
        margin-top: var(--spacing-xxs);
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

    /* The lead is CommonMark block HTML: its top-level paragraphs and lists stack with one
       gap between them and no outer margin, so the heading above and the toggle below keep
       their own spacing whatever block the lead starts or ends with. */
    .lead > :global(*) {
        margin: 0;
    }

    .lead > :global(*) + :global(*) {
        margin-top: var(--spacing-md);
    }

    /* Lists sit flush with the surrounding text: the marker occupies its own column, so a
       wrapped line lines up under the first line instead of under the bullet. Applies to a
       lead's authored lists (nested ones too) and the entry lists alike. */
    .lead :global(ul),
    .lead :global(ol),
    .entries {
        padding: 0;
        list-style: none;
    }

    /* Top margin only: the block below owns its own leading, so a list can't stack two gaps
       at the end of a section. */
    .entries {
        margin: var(--spacing-md) 0 0;
    }

    /* A nested list hangs under its parent item's text, as tight as the items around it. */
    .lead :global(li) > :global(ul),
    .lead :global(li) > :global(ol) {
        margin: var(--spacing-xs) 0 0;
    }

    /* A loose list (blank lines between items) wraps each item's text in a <p>. The first
       one stays inline (where a vertical margin does nothing), so the marker keeps sharing
       its line. */
    .lead :global(li) > :global(p) {
        margin: var(--spacing-xs) 0 0;
    }

    .lead :global(li) > :global(p:first-child) {
        display: inline;
    }

    /* A hanging indent, ❌ never a two-column grid. An entry is rendered markdown, and a
       grid makes every inline element (<code>, <strong>, a link) its own ITEM: the text
       splits across cells and the markup auto-places into the marker column, too narrow
       for a word, which then breaks one character per line. Padding plus a negative
       margin on the marker keeps the whole entry in one inline flow, wrapped lines still
       hanging under the first. `whats-new-markup.spec.ts` measures it. */
    .lead :global(li),
    .entries li {
        padding-left: var(--spacing-whats-new-marker);
    }

    .lead :global(li + li),
    .entries li + li {
        margin-top: var(--spacing-xs);
    }

    /* Inline-block for a box the negative margin can act on; the glyph's own baseline
       keeps it on the text's first-line baseline. The indent is an `em` of the ITEM's font
       size, but inside the marker `em` resolves against the marker's enlarged size, so the
       scale divides back out to land on the same width. */
    .lead :global(li)::before,
    .entries li::before {
        display: inline-block;
        width: calc(var(--spacing-whats-new-marker) / var(--font-whats-new-marker-scale));
        margin-left: calc(-1 * var(--spacing-whats-new-marker) / var(--font-whats-new-marker-scale));
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
        padding-left: 1.6em;
    }

    /* A text-size marker, so its own `em` needs no scale divided out. */
    .lead :global(ol) > :global(li)::before {
        counter-increment: lead-item;
        content: counter(lead-item) '.';
        width: 1.6em;
        margin-left: -1.6em;
    }

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
        font-size: calc(1em * var(--font-whats-new-marker-scale));
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

    /* A link inside an entry or a lead. Same accent and underline as the dialog's own
       links, and the pointer has to be re-enabled over the app-wide `cursor: default`. */
    .entries li :global(a),
    .lead :global(a) {
        color: var(--color-accent-text);
        text-decoration: underline;
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- re-enable pointer on links over the global cursor: default (see above) */
        cursor: pointer;
    }

    .entries li :global(a:hover),
    .lead :global(a:hover) {
        color: var(--color-accent-hover);
    }

    .full-changelog {
        margin: var(--spacing-md) 0 0;
        font-size: var(--font-size-md);
    }
</style>
