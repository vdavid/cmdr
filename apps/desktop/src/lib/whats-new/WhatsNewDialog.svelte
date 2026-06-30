<script lang="ts">
    /**
     * Post-update "What's new" popup: renders the changelog slice (lead + Added / Changed /
     * Fixed / Security sections) for the releases between the version the user last saw and
     * the one running now. Driven by the reactive `whatsNewState`; mounted from
     * `routes/(main)/+page.svelte` while `whatsNewState.open` is true.
     *
     * The auto-popup never opens empty (the trigger collapses an empty slice to a silent
     * stamp); the empty state is reachable only via the manual Help reopen.
     */
    import snarkdown from 'snarkdown'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
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
    containerStyle="width: 560px; max-width: calc(100vw - 2 * var(--spacing-xl))"
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
                            <!-- eslint-disable-next-line svelte/no-at-html-tags -- trusted: renders our committed CHANGELOG via renderMarkdown(), not user input -->
                            <p class="lead">{@html renderMarkdown(release.lead)}</p>
                        {/if}
                        {#each release.sections as section (section.title)}
                            <h4 class="section-title">{section.title}</h4>
                            <ul class="entries">
                                {#each section.entries as entry, i (i)}
                                    <!-- eslint-disable-next-line svelte/no-at-html-tags -- trusted: renders our committed CHANGELOG via renderMarkdown(), not user input -->
                                    <li>{@html renderMarkdown(entry)}</li>
                                {/each}
                            </ul>
                        {/each}
                    </section>
                {/each}
            {/if}

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

        <div class="footer">
            <Button variant="secondary" onclick={handleOptOut}>{tString('whatsNew.dialog.optOut')}</Button>
            <Button variant="primary" onclick={handleClose}>{tString('whatsNew.dialog.close')}</Button>
        </div>
    </div>
</ModalDialog>

<style>
    .body {
        display: flex;
        flex-direction: column;
        padding: 0 var(--spacing-xl) var(--spacing-xl);
        /* Cap the dialog height to the window so a long slice scrolls instead of overflowing. */
        max-height: calc(100vh - 2 * var(--spacing-2xl) - var(--titlebar-height));
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
        line-height: 1.5;
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

    .version {
        font-size: var(--font-size-lg);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .dot,
    .date {
        font-size: var(--font-size-sm);
        font-weight: 400;
        color: var(--color-text-tertiary);
    }

    .lead {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.55;
    }

    .section-title {
        margin: var(--spacing-md) 0 var(--spacing-xs);
        font-size: var(--font-size-sm);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .entries {
        margin: 0;
        padding-left: var(--spacing-lg);
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    .entries li {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        line-height: 1.5;
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
        font-size: var(--font-size-sm);
    }

    .footer {
        display: flex;
        align-items: center;
        justify-content: flex-end;
        gap: var(--spacing-md);
        margin-top: var(--spacing-lg);
        padding-top: var(--spacing-md);
        border-top: 1px solid var(--color-border);
    }
</style>
