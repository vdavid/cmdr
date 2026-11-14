<!--
  The "Tell me about this" card: a resizable dialog that renders one `AboutCardData` as it fills. The title, then the
  facts (the headline with two lines reserved, the volume note, the evidence sections in the plan's order) scrolling
  above What removing it means, which stays pinned in view. With an AI take, that's the left column and the take the
  right one (below, on a narrow card). The footer stays put, with the AI hint line under it when a setting stands
  between the card and a take.

  Pure rendering: the live slice and the gallery both hand it a card built by `applyAboutEvent`. Closing is the
  host's (`onClose`), which cancels the request.
-->
<script lang="ts">
    import { onMount } from 'svelte'
    import type { AboutAction, AboutTarget } from '#lib/ipc/bindings.js'
    import { t, tString } from '#lib/intl/messages.svelte.js'
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import Button from '#lib/ui/Button.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { getCachedCustomFolderIcon, getCachedIcon, iconCacheVersion } from '#lib/icon-cache.js'
    import type { IconName } from '#lib/ui/icons/icon-map.js'
    import AboutActions from './AboutActions.svelte'
    import AboutHeadline from './AboutHeadline.svelte'
    import AboutRemoval from './AboutRemoval.svelte'
    import AboutRow from './AboutRow.svelte'
    import AboutVerdict from './AboutVerdict.svelte'
    import AboutAiHint from './AboutAiHint.svelte'
    import { hintWhy, verdictPlace } from './about-verdict'
    import { useInlineSize } from '#lib/utils/inline-size-action.js'
    import type { AboutCardData, AboutGoTarget } from './about-card'
    import { collapsedLine, sectionsOf } from './about-card-view'
    import { SECTION_TITLE, UNAVAILABLE, VOLUME_NOTE } from './about-words'

    interface Props {
        card: AboutCardData
        onClose: () => void
        /** Enter on a row that names a place (a folder's child, an app's Library folder): close, then go there. */
        onGoTo: (target: AboutGoTarget) => void
        /** A footer action: close, then do it. */
        onAct: (action: AboutAction) => void
        /** The AI take's Stop: stops it alone, and the card stays. */
        onStopVerdict: () => void
        /** The person closed the AI hint line before (`aboutThis.aiHintDismissed`). */
        aiHintDismissed: boolean
        /** The hint line's ×: remember it, for every card. */
        onDismissAiHint: () => void
    }

    const { card, onClose, onGoTo, onAct, onStopVerdict, aiHintDismissed, onDismissAiHint }: Props = $props()

    const TARGET_ICON: Record<AboutTarget, IconName> = {
        file: 'file',
        folder: 'folder',
        appBundle: 'monitor',
        package: 'folder',
        symlink: 'link',
        archiveEntry: 'file-archive',
    }

    const target = $derived(card.plan?.target ?? 'file')
    /**
     * The icon the pane shows for the item (an app's own, a folder's custom one), from the icon cache the pane
     * already filled. ❌ Never a fetch from here: a miss draws the glyph.
     */
    const iconUrl = $derived.by(() => {
        // eslint-disable-next-line @typescript-eslint/no-unused-expressions -- reactive read: a late icon redraws the title.
        $iconCacheVersion
        const { iconId, path } = card.subject
        return getCachedCustomFolderIcon(path) ?? (iconId !== null ? getCachedIcon(iconId) : undefined)
    })
    const sections = $derived(sectionsOf(card))

    /**
     * Lines What's inside can take: its summary (two), the biggest children and the largest files (eight each,
     * `cmdr_about::digest::SHOWN_CHILDREN`), and the untouched share. Held until the facts settle, so a digest landing
     * after the first frame (its searches scan the whole arena: hundreds of ms on a big drive) moves nothing below.
     */
    const CONTENTS_RESERVED_LINES = 19
    const name = $derived.by(() => {
        const identity = card.facts.identity
        return identity?.kind === 'identity' && identity.fact.state === 'known' ? identity.fact.value.name : card.subject.name
    })

    let body: HTMLElement | undefined = $state()
    /** Said once, politely, when the facts settle: the headline's words. ❌ No live region per fact. */
    let announcement = $state('')

    $effect(() => {
        if (card.settled && !card.unavailable) announcement = body?.querySelector('[data-headline]')?.textContent.trim() ?? ''
    })

    /**
     * The AI take, said once, politely, when it ends (or arrives whole from earlier). ❌ No live region over the
     * stream: it would read every delta.
     */
    let verdictAnnouncement = $state('')
    $effect(() => {
        const verdict = card.verdict
        if (verdict?.status === 'planned' && verdict.ending !== null && verdict.text.trim() !== '') {
            verdictAnnouncement = tString('aboutThis.verdict.announcement', { text: verdict.text })
        }
    })

    /** Where the AI take goes: its own column, a hint line under the footer, or nowhere. */
    const place = $derived(verdictPlace(card.verdict))
    const hint = $derived(card.unavailable || aiHintDismissed ? null : hintWhy(card.verdict))
    /** The take has a column only on a card with facts. */
    const withTake = $derived(place === 'column' && !card.unavailable)

    /**
     * Two columns need about 46 rem for the facts (an 8.5 rem label column and room for a path) beside a take that
     * reads in lines of a few words. Narrower (a small window, or the card dragged narrow), the take goes below.
     */
    const TWO_COLUMNS_REM = 46
    let layoutWidth = $state(0)
    const stacked = $derived(withTake && layoutWidth > 0 && layoutWidth < TWO_COLUMNS_REM * rootFontSize())

    function rootFontSize(): number {
        return Number.parseFloat(getComputedStyle(document.documentElement).fontSize) || 16
    }

    /** A take longer than its column scrolls, and only then is the column a Tab stop, so the keys can scroll it. */
    let takeColumn: HTMLElement | undefined = $state()
    let takeOverflows = $state(false)
    $effect(() => {
        const column = takeColumn
        if (!column) return
        const measure = (): void => {
            takeOverflows = column.scrollHeight > column.clientHeight + 1
        }
        const observer = new ResizeObserver(measure)
        observer.observe(column)
        for (const child of column.children) observer.observe(child)
        measure()
        return () => {
            observer.disconnect()
        }
    })

    onMount(() => {
        // The body, so the arrow keys and Page Up/Down scroll the card from the first keystroke.
        body?.focus({ preventScroll: true })
    })
</script>

<!-- A setting stands between the card and an AI take: one line under the buttons, until its × hides it for good. -->
{#snippet aiHintNote()}
    {#if hint}<AboutAiHint why={hint} onDismiss={onDismissAiHint} />{/if}
{/snippet}

<ModalDialog
    footerNote={hint ? aiHintNote : undefined}
    titleId="about-this-title"
    dialogId="tell-me-about-this"
    resizable
    growDownward
    fillBody
    onclose={onClose}
    containerStyle={withTake ? 'width: 60rem; max-height: 85vh;' : 'width: 40rem; max-height: 85vh;'}
>
    {#snippet title()}
        <span class="title"
            ><span class="title-icon" aria-hidden="true"
                >{#if iconUrl}<img src={iconUrl} alt="" width="16" height="16" />{:else}<Icon name={TARGET_ICON[target]} size={16} />{/if}</span
            ><span class="title-text" use:tooltip={name}
                >{t('aboutThis.dialog.title', { name })}</span
            ></span
        >
    {/snippet}

    <!--
        Left (or on top): everything Cmdr knows by itself, its facts scrolling and What removing it means pinned under
        them, outside the scroller, so it's in view whenever the take is. Right (or below, on a narrow card): the take.
        DOM order is the reading and Tab order: facts, removal, then the take.
    -->
    <div
        class="layout"
        class:two-columns={withTake && !stacked}
        class:stacked
        data-layout={withTake ? (stacked ? 'stacked' : 'twoColumns') : 'oneColumn'}
        use:useInlineSize={{ onResize: (px) => (layoutWidth = px) }}
    >
        <div class="facts-column">
            <div class="card" tabindex="0" bind:this={body} role="document" aria-labelledby="about-this-title" data-settled={card.settled}>
                {#if card.unavailable}
                    <p class="unavailable">{t(UNAVAILABLE[card.unavailable])}</p>
                {:else}
                    <AboutHeadline headline={card.derived?.headline ?? null} settled={card.settled} />

                    {#if card.plan?.volumeNote}
                        <p class="volume-note">
                            {t(VOLUME_NOTE[card.plan.volumeNote.limit], { volume: card.plan.volumeNote.volume })}
                        </p>
                    {/if}

                    {#each sections as view (view.section)}
                        <section class="section" aria-labelledby="about-section-{view.section}" data-section={view.section}>
                            <h3 id="about-section-{view.section}">{t(SECTION_TITLE[view.section])}</h3>
                            {#if view.collapsed}
                                <p class="quiet">{t(collapsedLine(view))}</p>
                            {:else}
                                <dl
                                    style:min-height={view.section === 'contents' && !card.settled
                                        ? `calc(${String(CONTENTS_RESERVED_LINES)} * var(--font-line-height-normal) * 1em)`
                                        : null}
                                >
                                    {#each view.rows as row (row.kind)}
                                        <AboutRow {row} {target} settled={card.settled} subjectPath={card.subject.path} {onGoTo} />
                                    {/each}
                                </dl>
                            {/if}
                        </section>
                    {/each}
                {/if}
            </div>

            {#if card.plan && !card.unavailable}
                <section class="section removal" aria-labelledby="about-section-removal" data-removal>
                    <h3 id="about-section-removal">{t(SECTION_TITLE.removal)}</h3>
                    <AboutRemoval removal={card.derived?.removal ?? null} settled={card.settled} />
                </section>
            {/if}
        </div>

        {#if withTake && card.verdict}
            <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
            <div
                class="take-column"
                bind:this={takeColumn}
                data-take-column
                role="region"
                aria-label={tString('aboutThis.verdict.column')}
                tabindex={takeOverflows ? 0 : undefined}
            >
                <AboutVerdict verdict={card.verdict} onStop={onStopVerdict} />
            </div>
        {/if}
    </div>
    <p class="sr-only" aria-live="polite">{announcement}</p>
    <p class="sr-only" aria-live="polite">{verdictAnnouncement}</p>

    {#snippet footerLeading()}
        <AboutActions actions={card.unavailable ? null : (card.derived?.actions ?? null)} {onAct} />
    {/snippet}
    {#snippet footer()}
        <Button variant="secondary" onclick={onClose}>{t('aboutThis.dialog.close')}</Button>
    {/snippet}
</ModalDialog>

<style>
    .title {
        display: flex;
        gap: var(--spacing-sm);
        align-items: center;
        min-width: 0;
    }

    .title-icon {
        flex: none;
        display: inline-flex;
        color: var(--color-text-secondary);
    }

    .title-text {
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    /* The body's one child: the columns, side by side or stacked, in the height `fillBody` gives. */
    .layout {
        display: flex;
        flex: 1 1 auto;
        gap: var(--spacing-xl);
        min-height: 0;
        font-size: var(--font-size-sm);
    }

    .layout.stacked {
        flex-direction: column;
        gap: var(--spacing-lg);
    }

    .facts-column {
        display: flex;
        flex: 1 1 auto;
        flex-direction: column;
        gap: var(--spacing-lg);
        min-width: 0;
        min-height: 0;
    }

    .layout.two-columns .facts-column {
        flex: 3 1 0;
    }

    /* The facts scroll. Content-sized until the panel runs out of height, so What removing it means sits right under
       them on a short card and stays in view on a long one. A reserved gutter, pulled into the body's inset where the
       scroller meets the panel's edge (#889, `#lib/ui/DETAILS.md` § "Scrollbar gutters"). */
    .card {
        display: flex;
        flex: 0 1 auto;
        flex-direction: column;
        gap: var(--spacing-lg);
        min-height: 0;
        overflow-y: auto;
        scrollbar-gutter: stable;
        margin-right: calc(var(--spacing-scrollbar-width) * -1);
        outline: none;
    }

    .layout.two-columns .card {
        margin-right: 0;
    }

    /* Pinned under the facts, never inside their scroller. Capped so a long list behind "N more" can't push the facts
       out of the column; past that it scrolls on its own. */
    .removal {
        flex: none;
        max-height: 50%;
        overflow-y: auto;
        padding-top: var(--spacing-md);
        border-top: 1px solid var(--color-border-subtle);
    }

    .take-column {
        flex: 2 1 0;
        min-width: 0;
        min-height: 0;
        overflow-y: auto;
        scrollbar-gutter: stable;
        margin-right: calc(var(--spacing-scrollbar-width) * -1);
        outline: none;
    }

    /* Below the facts on a narrow card: its own height, up to a share of the window, so the facts and the removal row
       above it keep their room. */
    .layout.stacked .take-column {
        flex: 0 1 auto;
        max-height: 40vh;
    }

    .take-column:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
    }

    .volume-note,
    .unavailable {
        margin: 0;
        color: var(--color-text-secondary);
    }

    .section {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    h3 {
        margin: 0;
        font-size: var(--font-size-sm);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    dl {
        margin: 0;
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    .quiet {
        margin: 0;
        color: var(--color-text-tertiary);
    }
</style>
