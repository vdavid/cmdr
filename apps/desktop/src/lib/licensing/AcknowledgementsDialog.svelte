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

    /** Scrolls the npm heading to the top of the body's scroll region. */
    function jumpToNpm(): void {
        const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches
        headings.npm?.scrollIntoView({ block: 'start', behavior: reduceMotion ? 'auto' : 'smooth' })
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
    padded={false}
    containerStyle="width: 644px; min-width: 644px; max-width: 644px; height: 80vh"
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

        <hr class="note-divider" />

        {#if loaded}
            <div class="jump">
                <Button size="mini" onclick={jumpToNpm}>{tString('licensing.acknowledgements.jumpToNpm')}</Button>
            </div>

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

            <p class="full-texts"><Trans key="licensing.acknowledgements.fullTexts" snippets={{ notices }} /></p>
        {:else}
            <p class="loading"><Spinner size="sm" /> {tString('licensing.acknowledgements.loading')}</p>
        {/if}
    </div>
</ModalDialog>

<style>
    /* The body IS the scroll region (`fillBody` + `padded={false}`): it absorbs the
       panel's vertical slack, so the list reaches the bottom edge at any content
       length, and it owns the padding ModalDialog would otherwise apply, so the
       scrollbar rides the panel edge. The horizontal inset is `--spacing-dialog`,
       the title bar's own inset, which lines the content up with the title. */
    .acknowledgements-body {
        flex: 1 1 auto;
        min-height: 0;
        overflow-y: auto;
        padding: 0 var(--spacing-dialog) var(--spacing-dialog);
    }

    .note p {
        margin: 0 0 var(--spacing-sm);
        color: var(--color-text-primary);
        font-size: var(--font-size-md);
        line-height: 1.5;
    }

    .signature {
        color: var(--color-text-secondary);
    }

    .note-divider {
        margin: var(--spacing-lg) 0;
        border: none;
        border-top: 1px solid var(--color-border-subtle);
    }

    .jump {
        margin-bottom: var(--spacing-sm);
    }

    /* One grid for BOTH lists: each `ul` and each row is a `subgrid`, so the three
       columns are measured once across every package and the crate list and the npm
       list line up with each other. The name track takes the slack; version and
       license size to their content. */
    .packages {
        display: grid;
        grid-template-columns: minmax(0, 1fr) auto auto;
        column-gap: var(--spacing-lg);
    }

    .packages > h3,
    .packages > .package-list {
        grid-column: 1 / -1;
    }

    h3 {
        margin: var(--spacing-xl) 0 var(--spacing-sm);
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
        padding: var(--spacing-xxs) 0;
        font-size: var(--font-size-sm);
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

    .package-version,
    .package-license {
        color: var(--color-text-tertiary);
        white-space: nowrap;
    }

    .full-texts,
    .loading {
        margin: var(--spacing-xl) 0 0;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }

    .loading {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }
</style>
