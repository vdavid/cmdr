<!--
  The "Tell me about this" card: a resizable dialog that renders one `AboutCardData` as it fills. Top to bottom: the
  title, the headline (two lines reserved), the volume note, the evidence sections in the plan's order, an empty slot
  for the verdict, What removing it means, and a footer that stays put while the body scrolls.

  Pure rendering: the live slice and the gallery both hand it a card built by `applyAboutEvent`. Closing is the
  host's (`onClose`), which cancels the request.
-->
<script lang="ts">
    import { onMount } from 'svelte'
    import type { AboutTarget } from '#lib/ipc/bindings.js'
    import { t } from '#lib/intl/messages.svelte.js'
    import ModalDialog from '#lib/ui/ModalDialog.svelte'
    import Button from '#lib/ui/Button.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { getCachedCustomFolderIcon, getCachedIcon, iconCacheVersion } from '#lib/icon-cache.js'
    import type { IconName } from '#lib/ui/icons/icon-map.js'
    import AboutHeadline from './AboutHeadline.svelte'
    import AboutRemoval from './AboutRemoval.svelte'
    import AboutRow from './AboutRow.svelte'
    import type { AboutCardData, AboutGoTarget } from './about-card'
    import { sectionsOf } from './about-card-view'
    import { SECTION_TITLE, UNAVAILABLE, UNKNOWN_WHY, VOLUME_NOTE } from './about-words'

    interface Props {
        card: AboutCardData
        onClose: () => void
        /** Enter on a row that names a place (a folder's child, an app's Library folder): close, then go there. */
        onGoTo: (target: AboutGoTarget) => void
    }

    const { card, onClose, onGoTo }: Props = $props()

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

    onMount(() => {
        // The body, so the arrow keys and Page Up/Down scroll the card from the first keystroke.
        body?.focus({ preventScroll: true })
    })
</script>

<ModalDialog
    titleId="about-this-title"
    dialogId="tell-me-about-this"
    resizable
    growDownward
    onclose={onClose}
    containerStyle="width: 34rem; max-height: 85vh;"
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
                        <dl>
                            {#each view.rows as row (row.kind)}
                                <AboutRow {row} {target} settled={card.settled} subjectPath={card.subject.path} {onGoTo} />
                            {/each}
                        </dl>
                    {/if}
                </section>
            {/each}

            <!-- The verdict's place: right above What removing it means, empty and zero-height until a verdict is planned. -->
            <div class="verdict-slot" data-verdict-slot></div>

            {#if card.plan}
                <section class="section" aria-labelledby="about-section-removal">
                    <h3 id="about-section-removal">{t(SECTION_TITLE.removal)}</h3>
                    <AboutRemoval removal={card.derived?.removal ?? null} settled={card.settled} />
                </section>
            {/if}
        {/if}
    </div>
    <p class="sr-only" aria-live="polite">{announcement}</p>

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

    .verdict-slot:empty {
        display: none;
    }
</style>
