<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Dropdown from '$lib/ui/Dropdown.svelte'

    let anchorEl: HTMLButtonElement | undefined = $state()
    let open = $state(false)
</script>

<SectionCard id="components-dropdown" label="Dropdown">
    <div class="cell">
        <p class="caption">
            Generic positioned floater: frosted glass, auto-flip, focus trap, Esc closes. Click the anchor to toggle.
        </p>
        <button
            bind:this={anchorEl}
            type="button"
            class="demo-anchor"
            onclick={() => {
                open = !open
            }}
        >
            {open ? 'Close dropdown' : 'Open dropdown'}
        </button>
        {#if anchorEl}
            <Dropdown
                anchor={anchorEl}
                {open}
                onClose={() => {
                    open = false
                }}
                ariaLabel="Demo dropdown"
            >
                <div class="demo-content">
                    <p>Any content goes here.</p>
                    <input type="text" aria-label="Demo field" placeholder="Type here" />
                </div>
            </Dropdown>
        {/if}
    </div>
</SectionCard>

<style>
    .caption {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .demo-anchor {
        padding: var(--spacing-xs) var(--spacing-md);
        font-size: var(--font-size-sm);
        background: transparent;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
    }

    .demo-content {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        min-width: 200px;
    }

    .demo-content input {
        padding: var(--spacing-xs) var(--spacing-sm);
        font-size: var(--font-size-sm);
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
    }
</style>
