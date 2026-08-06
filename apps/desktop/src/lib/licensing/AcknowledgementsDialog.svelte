<script lang="ts">
    import { onMount } from 'svelte'
    import { openExternalUrl } from '$lib/tauri-commands'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    /** The repo's notices file on GitHub; the full license texts also ship inside the app bundle. */
    const NOTICES_URL = 'https://github.com/vdavid/cmdr/blob/main/THIRD-PARTY-NOTICES.md'

    interface Props {
        onClose: () => void
    }

    const { onClose }: Props = $props()

    interface AttributedPackage {
        name: string
        version: string
        license: string
        url: string
    }

    let rust = $state<AttributedPackage[]>([])
    let npm = $state<AttributedPackage[]>([])
    let loaded = $state(false)
    /** Section headings by section key, so the jump button can scroll one into view. */
    const headings = $state<Record<string, HTMLElement | undefined>>({})

    onMount(async () => {
        // Loaded on open, not at startup: the list is ~119 KB of generated JSON
        // and nothing else in the app needs it. Vite code-splits the import.
        const packages = await import('./third-party-packages.gen.json')
        rust = packages.default.rust
        npm = packages.default.npm
        loaded = true
    })

    // Two lists, same markup. Iterating sections rather than calling a snippet
    // twice keeps this to one `{#each}` and avoids a second component (every
    // `lib/` component needs its own a11y test).
    const sections = $derived([
        { key: 'rust', heading: tString('licensing.acknowledgements.rustHeading', { count: rust.length }), packages: rust },
        { key: 'npm', heading: tString('licensing.acknowledgements.npmHeading', { count: npm.length }), packages: npm },
    ])

    function handleLinkClick(url: string) {
        return (event: MouseEvent) => {
            event.preventDefault()
            void openExternalUrl(url)
        }
    }

    /** Scrolls a section's heading to the top of the package list's scroll region. */
    function jumpTo(key: string): () => void {
        return () => {
            const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches
            headings[key]?.scrollIntoView({ block: 'start', behavior: reduceMotion ? 'auto' : 'smooth' })
        }
    }
</script>

{#snippet notices(children: import('svelte').Snippet)}
    <LinkButton
        href={NOTICES_URL}
        target="_blank"
        rel="noopener noreferrer"
        onclick={handleLinkClick(NOTICES_URL)}>{@render children()}</LinkButton
    >
{/snippet}

<ModalDialog
    titleId="acknowledgements-title"
    blur
    dialogId="acknowledgements"
    onclose={onClose}
    fillBody
    containerStyle="width: 515px; min-width: 515px; max-width: 515px; height: 80vh"
>
    {#snippet title()}
        <span id="acknowledgements-title">{tString('licensing.acknowledgements.srTitle')}</span>
    {/snippet}

    <div class="acknowledgements-body">
        <div class="note">
            <p>{tString('licensing.acknowledgements.note')}</p>
            <p>{tString('licensing.acknowledgements.thanks')}</p>
            <p class="signature">{tString('licensing.acknowledgements.signature')}</p>
        </div>

        {#if loaded}
            <div class="jump">
                <Button size="mini" onclick={jumpTo('rust')}>{tString('licensing.acknowledgements.jumpToRust')}</Button>
                <Button size="mini" onclick={jumpTo('npm')}>{tString('licensing.acknowledgements.jumpToNpm')}</Button>
            </div>

            <div class="packages-scroll">
                <div class="packages">
                    {#each sections as section (section.key)}
                        <h3 bind:this={headings[section.key]}>{section.heading}</h3>
                        <ul class="package-list">
                            {#each section.packages as pkg (pkg.name + pkg.version)}
                                <li>
                                    {#if pkg.url}
                                        <!-- `LinkButton` with an `href`: it owns the app's only sanctioned
                                             `cursor: pointer`, and its click handler routes to the system
                                             browser via the opener plugin instead of navigating the webview. -->
                                        <LinkButton
                                            href={pkg.url}
                                            target="_blank"
                                            rel="noopener noreferrer"
                                            onclick={handleLinkClick(pkg.url)}>{pkg.name}</LinkButton
                                        >
                                    {:else}
                                        <span class="package-name">{pkg.name}</span>
                                    {/if}
                                    <span class="package-version">{pkg.version}</span>
                                    <span class="package-license">{pkg.license}</span>
                                </li>
                            {/each}
                        </ul>
                    {/each}
                </div>
            </div>

            <p class="full-texts"><Trans key="licensing.acknowledgements.fullTexts" snippets={{ notices }} /></p>
        {:else}
            <p class="loading"><Spinner size="sm" /> {tString('licensing.acknowledgements.loading')}</p>
        {/if}
    </div>
</ModalDialog>

<style>
    /* A three-part column inside the `fillBody` panel: the thank-you note and the jump
       buttons on top, the notices link pinned at the bottom, and only the package list
       between them scrolling. Scrolling the whole body instead would push David's note
       and the notices link out of sight the moment you move the list. */
    .acknowledgements-body {
        flex: 1 1 auto;
        min-height: 0;
        display: flex;
        flex-direction: column;
    }

    .note p {
        margin: 0 0 var(--spacing-sm);
        color: var(--color-text-primary);
        font-size: var(--font-size-md);
        line-height: var(--font-line-height-prose);
    }

    .signature {
        color: var(--color-text-secondary);
    }

    /* Both jump buttons on one full-width row, half each: two equal targets read as a
       pair of section tabs, which is what they are. */
    .jump {
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: var(--spacing-md);
        margin-bottom: var(--spacing-sm);
    }

    .jump :global(.btn) {
        width: 100%;
    }

    /* The scroll region. Pulled out by the rows' own inset and padded back, so the
       striped rows line up with the headings and the dialog title while the scrollbar
       rides just inside the panel edge. `.packages`' negative margin lands its margin
       box exactly on this padding box, so nothing overflows sideways. */
    .packages-scroll {
        flex: 1 1 auto;
        min-height: 0;
        overflow-y: auto;
        margin-inline: calc(var(--spacing-sm) * -1);
        padding-inline: var(--spacing-sm);
    }

    /* One grid for BOTH lists: each `ul` and each row is a `subgrid`, so the three
       columns are measured once across every package and the crate list and the npm
       list line up with each other. The name track takes the slack; version and
       license size to their content. */
    .packages {
        display: grid;
        /* Version and license are CAPPED, not `auto`. Sized to content they were
           driven by the worst case (aws-lc-sys carries a ~130-char SPDX `AND`/`OR`
           expression), which starved the name track down to a few percent. Each
           track still shrinks below its cap when the content is shorter. */
        grid-template-columns: minmax(0, 1fr) minmax(0, 10%) minmax(0, 30%);
        column-gap: var(--spacing-lg);
        /* Pulled out so the striped rows' own inset lands their text back in line
           with the headings and the dialog title. */
        margin-inline: calc(var(--spacing-sm) * -1);
    }

    /* The first heading opens the scroll region, so it doesn't need the gap that
       separates the second list from the first one above it. */
    .packages > h3:first-child {
        margin-top: 0;
    }

    .packages > h3,
    .packages > .package-list {
        grid-column: 1 / -1;
    }

    h3 {
        /* Matches the rows' horizontal inset, cancelling `.packages`' negative
           margin so headings and rows share a left edge. */
        margin: var(--spacing-xl) var(--spacing-sm) var(--spacing-sm);
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .package-list {
        display: grid;
        grid-template-columns: subgrid;
        list-style: none;
        margin: 0;
        padding: 0;
    }

    .package-list li {
        grid-column: 1 / -1;
        display: grid;
        grid-template-columns: subgrid;
        align-items: baseline;
        padding: var(--spacing-xxs) var(--spacing-sm);
        border-radius: var(--radius-sm);
        font-size: var(--font-size-sm);
    }

    /* Zebra striping, to keep the eye on one row across three columns.
       `--color-bg-stripe` is translucent by design, so it composites over
       whatever sits behind the dialog and works in both modes. */
    .package-list li:nth-child(even) {
        background: var(--color-bg-stripe);
    }

    /* Resting rows stay clean: hundreds of permanently underlined names read as
       noise. The accent color and the pointer cursor still say "link"; the
       underline comes back on hover. */
    .package-list li :global(.link-button) {
        min-width: 0;
        overflow-wrap: anywhere;
        text-align: left;
        text-decoration: none;
    }

    .package-list li :global(.link-button:hover) {
        text-decoration: underline;
    }

    .package-name {
        min-width: 0;
        overflow-wrap: anywhere;
        color: var(--color-text-primary);
    }

    /* No `nowrap` here: it's what made the caps above unhonorable, since a long
       license had to claim a single line's worth of width. Long SPDX expressions
       wrap within their track instead. */
    .package-version,
    .package-license {
        min-width: 0;
        overflow-wrap: anywhere;
        color: var(--color-text-tertiary);
    }

    /* Pinned below the scroll region, never scrolled away: it's the pointer to the
       full license texts, which is the legally load-bearing part of this dialog. */
    .full-texts,
    .loading {
        margin: var(--spacing-md) 0 0;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }

    .loading {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }
</style>
