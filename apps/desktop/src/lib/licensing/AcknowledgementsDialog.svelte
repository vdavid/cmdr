<script lang="ts">
    import { onMount } from 'svelte'
    import { openExternalUrl } from '$lib/tauri-commands'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { tString } from '$lib/intl/messages.svelte'

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

    function openUrl(url: string): void {
        void openExternalUrl(url)
    }
</script>

<ModalDialog
    titleId="acknowledgements-title"
    blur
    dialogId="acknowledgements"
    onclose={onClose}
    resizable
    containerStyle="min-width: 460px; max-width: 640px"
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
            {#each sections as section (section.key)}
                <h3>{section.heading}</h3>
                <ul class="package-list">
                    {#each section.packages as pkg (pkg.name + pkg.version)}
                        <li>
                            {#if pkg.url}
                                <!-- A button, not an <a>: this opens the browser via the
                                     opener plugin rather than navigating the webview. -->
                                <button type="button" class="package-link" onclick={() => { openUrl(pkg.url); }}>
                                    {pkg.name}
                                </button>
                            {:else}
                                <span class="package-name">{pkg.name}</span>
                            {/if}
                            <span class="package-version">{pkg.version}</span>
                            <span class="package-license">{pkg.license}</span>
                        </li>
                    {/each}
                </ul>
            {/each}

            <p class="full-texts">{tString('licensing.acknowledgements.fullTexts')}</p>
        {:else}
            <p class="loading"><Spinner size="sm" /> {tString('licensing.acknowledgements.loading')}</p>
        {/if}
    </div>
</ModalDialog>

<style>
    .acknowledgements-body {
        padding: 0 var(--spacing-2xl) var(--spacing-2xl);
        max-height: 60vh;
        overflow-y: auto;
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

    h3 {
        margin: var(--spacing-xl) 0 var(--spacing-sm);
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .package-list {
        list-style: none;
        margin: 0;
        padding: 0;
    }

    .package-list li {
        display: flex;
        gap: var(--spacing-sm);
        align-items: baseline;
        padding: var(--spacing-xxs) 0;
        font-size: var(--font-size-sm);
    }

    .package-link {
        padding: 0;
        background: none;
        border: none;
        color: var(--color-accent-text);
        font: inherit;
        text-align: left;
    }

    .package-link:hover {
        text-decoration: underline;
    }

    .package-name {
        color: var(--color-text-primary);
    }

    .package-version,
    .package-license {
        color: var(--color-text-tertiary);
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
