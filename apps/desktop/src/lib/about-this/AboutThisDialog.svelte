<!--
  The "Tell me about this" card: a resizable dialog that renders one `AboutCardData` as it fills. Top to bottom: the
  title, the headline (two lines reserved), the volume note, the evidence sections in the plan's order, the AI take's
  block (only once it's planned), What removing it means, and a footer that stays put while the body scrolls.

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
    import { unavailableShowsALine, type AboutVerdict as AboutVerdictData } from './about-verdict'
    import LinkButton from '#lib/ui/LinkButton.svelte'
    import { openSettingsWindow } from '#lib/settings/settings-window.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import type { AboutCardData, AboutGoTarget } from './about-card'
    import { sectionsOf } from './about-card-view'
    import { SECTION_TITLE, UNAVAILABLE, UNKNOWN_WHY, VOLUME_NOTE } from './about-words'

    interface Props {
        card: AboutCardData
        onClose: () => void
        /** Enter on a row that names a place (a folder's child, an app's Library folder): close, then go there. */
        onGoTo: (target: AboutGoTarget) => void
        /** A footer action: close, then do it. */
        onAct: (action: AboutAction) => void
        /** The AI take's Stop: stops it alone, and the card stays. */
        onStopVerdict: () => void
    }

    const { card, onClose, onGoTo, onAct, onStopVerdict }: Props = $props()

    const log = getAppLogger('aboutThis')

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

    /** Whether the AI take draws in the body: a planned one always, an unavailable one only when it has a line. */
    function inBody(verdict: AboutVerdictData): boolean {
        return verdict.status === 'planned' || unavailableShowsALine(verdict.why)
    }

    /** With AI off, the card gets no AI take and says where to turn it on, quietly, in the footer. */
    const aiOff = $derived(card.verdict?.status === 'unavailable' && card.verdict.why.kind === 'aiOff')

    function openProviderSettings(): void {
        openSettingsWindow('about-this-card', ['AI', 'Provider']).catch((e: unknown) => {
            log.warn('opening the AI settings failed: {error}', { error: e })
        })
    }

    onMount(() => {
        // The body, so the arrow keys and Page Up/Down scroll the card from the first keystroke.
        body?.focus({ preventScroll: true })
    })
</script>

<!-- With AI off: one quiet line under the footer's buttons saying where to turn it on. -->
{#snippet aiOffNote()}
    <p class="ai-off">
        {tString('aboutThis.verdict.aiOffHint')}
        <LinkButton onclick={openProviderSettings}>{tString('aboutThis.verdict.unavailable.openAiSettings')}</LinkButton>
    </p>
{/snippet}

<ModalDialog
    footerNote={aiOff ? aiOffNote : undefined}
    titleId="about-this-title"
    dialogId="tell-me-about-this"
    resizable
    growDownward
    onclose={onClose}
    containerStyle="width: 40rem; max-height: 85vh;"
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
                        <p class="quiet">{t(view.collapsedWhy ? UNKNOWN_WHY[view.collapsedWhy] : 'aboutThis.section.nothingOnRecord')}</p>
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

            <!-- The AI take's place: right above What removing it means, and nothing at all without one. -->
            {#if card.verdict && inBody(card.verdict)}
                <div data-verdict-slot><AboutVerdict verdict={card.verdict} onStop={onStopVerdict} /></div>
            {/if}

            {#if card.plan}
                <section class="section" aria-labelledby="about-section-removal">
                    <h3 id="about-section-removal">{t(SECTION_TITLE.removal)}</h3>
                    <AboutRemoval removal={card.derived?.removal ?? null} settled={card.settled} />
                </section>
            {/if}
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

    .card {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-lg);
        font-size: var(--font-size-sm);
        outline: none;
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

    .ai-off {
        margin: 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }
</style>
