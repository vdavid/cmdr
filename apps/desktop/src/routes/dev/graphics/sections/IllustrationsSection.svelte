<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import GlobalShortcutAnimation from '$lib/downloads/GlobalShortcutAnimation.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'

    const KEYBOARD_USAGE = 'Plays in the download toast to teach the global go-to-latest-download shortcut (⌃⌥⌘J).'
    const NETWORK_USAGE = 'Empty state when no SMB hosts are found on the network.'
</script>

<SectionCard id="graphics-illustrations" label="Illustrations">
    <p class="intro">
        Larger decorative graphics: the keyboard animation that teaches a shortcut, and the empty-network illustration.
    </p>
    <div class="grid">
        <div class="cell" use:tooltip={KEYBOARD_USAGE}>
            <div class="illustration-host">
                <GlobalShortcutAnimation />
            </div>
            <p class="caption">Global shortcut keyboard</p>
        </div>
        <div class="cell" use:tooltip={NETWORK_USAGE}>
            <div class="illustration-host">
                <img src="/icons/network-no-hosts.svg" alt="" />
            </div>
            <p class="caption">Empty network</p>
        </div>
    </div>
</SectionCard>

<style>
    .intro {
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .grid {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
        gap: var(--spacing-lg);
    }

    .cell {
        display: flex;
        flex-direction: column;
        align-items: center;
    }

    .illustration-host {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 100%;
        height: 160px;
    }

    .illustration-host :global(svg),
    .illustration-host img {
        max-width: 100%;
        max-height: 100%;
    }

    .caption {
        margin: var(--spacing-md) 0 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        text-align: center;
    }
</style>
