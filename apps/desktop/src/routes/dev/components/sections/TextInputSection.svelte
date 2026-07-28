<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import TextArea from '$lib/ui/TextArea.svelte'

    let plain = $state('Documents')
    let search = $state('')
    let password = $state('hunter2')
    let revealed = $state(false)
    let invalid = $state('smb//nas.local')
    let notes = $state('Copy failed on 3 of 812 files.')

    const errorDetail = 'copy /a/b -> /c/d\n  ENOSPC: no space left on device'
</script>

<SectionCard id="components-text-input" label="Text input">
    <div class="grid">
        <div class="cell">
            <p class="caption">Default (radius <code>lg</code>, the house field)</p>
            <TextInput bind:value={plain} ariaLabel="Folder name" placeholder="Folder name" />
        </div>

        <div class="cell">
            <p class="caption">Placeholder only</p>
            <TextInput value="" ariaLabel="Empty field" placeholder="Type a path, like /Users/you/Documents" />
        </div>

        <div class="cell">
            <p class="caption">
                Leading icon + <code>radius="full"</code> + a trailing clear button (the search pill)
            </p>
            <TextInput
                bind:value={search}
                type="search"
                radius="full"
                leadingIcon="search"
                ariaLabel="Search settings"
                placeholder="Search"
            >
                {#snippet trailing()}
                    {#if search}
                        <button
                            class="affix-button"
                            type="button"
                            aria-label="Clear the search"
                            onclick={() => (search = '')}
                        >
                            ×
                        </button>
                    {/if}
                {/snippet}
            </TextInput>
        </div>

        <div class="cell">
            <p class="caption">Password with a reveal toggle in the trailing slot</p>
            <TextInput
                bind:value={password}
                type={revealed ? 'text' : 'password'}
                ariaLabel="API key"
                placeholder="sk-…"
                autocomplete="off"
                spellcheck={false}
            >
                {#snippet trailing()}
                    <button
                        class="affix-button"
                        type="button"
                        aria-label={revealed ? 'Hide value' : 'Show value'}
                        onclick={() => (revealed = !revealed)}
                    >
                        {revealed ? 'Hide' : 'Show'}
                    </button>
                {/snippet}
            </TextInput>
        </div>

        <div class="cell">
            <p class="caption">Invalid</p>
            <TextInput bind:value={invalid} invalid ariaLabel="Server address" />
        </div>

        <div class="cell">
            <p class="caption">Read-only / disabled</p>
            <div class="row">
                <TextInput value="/Volumes/naspi/papers" readonly ariaLabel="Read-only path" />
                <TextInput value="Not available" disabled ariaLabel="Disabled field" />
            </div>
        </div>

        <div class="cell">
            <p class="caption">
                Radius scale: <code>sm</code> / <code>md</code> / <code>lg</code> / <code>full</code>
            </p>
            <div class="row">
                <TextInput value="sm" radius="sm" ariaLabel="Radius sm" />
                <TextInput value="md" radius="md" ariaLabel="Radius md" />
                <TextInput value="lg" radius="lg" ariaLabel="Radius lg" />
                <TextInput value="full" radius="full" ariaLabel="Radius full" />
            </div>
        </div>

        <div class="cell">
            <p class="caption">
                Chromeless (inline rename, palette query lines): no frame, same caret, selection, and typography
            </p>
            <div class="chromeless-host">
                <TextInput value="report-2026.pdf" variant="chromeless" ariaLabel="Chromeless field" />
            </div>
        </div>

        <div class="cell">
            <p class="caption">Multi-line sibling (<code>TextArea</code>), same frame</p>
            <TextArea bind:value={notes} rows={4} ariaLabel="Notes" placeholder="What happened?" />
        </div>

        <div class="cell">
            <p class="caption">Read-only, non-resizable text area</p>
            <TextArea
                value={errorDetail}
                rows={3}
                readonly
                resizable={false}
                ariaLabel="Error detail"
            />
        </div>
    </div>
</SectionCard>

<style>
    .grid {
        display: grid;
        grid-template-columns: 1fr;
        gap: var(--spacing-lg);
    }

    .row {
        display: flex;
        align-items: center;
        gap: var(--spacing-md);
    }

    .caption {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .caption code {
        font-family: var(--font-mono);
    }

    /* Stand-in for a real host surface, so the chromeless field is visible. */
    .chromeless-host {
        padding: var(--spacing-sm);
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-sm);
    }

    /* Stand-in for the real trailing controls (`SettingsSidebar`'s clear button,
       `SettingPasswordInput`'s reveal toggle), which render an `Icon` here. The
       catalog uses text so `Icon` stays demoed in the Graphics catalog only. */
    .affix-button {
        display: flex;
        align-items: center;
        justify-content: center;
        min-width: 20px;
        height: 20px;
        padding: 0 var(--spacing-xxs);
        font-size: var(--font-size-xs);
        border: none;
        border-radius: var(--radius-sm);
        background: transparent;
        color: var(--color-text-tertiary);
    }

    .affix-button:hover {
        color: var(--color-text-primary);
        background: var(--color-bg-tertiary);
    }
</style>
