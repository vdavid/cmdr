<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Popover from '$lib/ui/Popover.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import DemoAnchor from '../DemoAnchor.svelte'

    let anchorEl: HTMLButtonElement | undefined = $state()
    let open = $state(false)
</script>

<SectionCard id="components-popover" label="Popover">
    <div class="cell">
        <p class="caption">
            Generic positioned floater: frosted glass, auto-flip, focus trap, Esc closes. Click the anchor to toggle.
        </p>
        <DemoAnchor
            bind:el={anchorEl}
            onclick={() => {
                open = !open
            }}
        >
            {open ? 'Close popover' : 'Open popover'}
        </DemoAnchor>
        {#if anchorEl}
            <Popover
                anchor={anchorEl}
                {open}
                onClose={() => {
                    open = false
                }}
                ariaLabel="Demo popover"
            >
                <div class="demo-content">
                    <p>Any content goes here.</p>
                    <TextInput ariaLabel="Demo field" placeholder="Type here" />
                </div>
            </Popover>
        {/if}
    </div>
</SectionCard>

<style>
    .caption {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .demo-content {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        min-width: 200px;
    }
</style>
