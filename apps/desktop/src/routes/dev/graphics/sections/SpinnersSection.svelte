<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'

    const SM_USAGE =
        'Inline and in-field loaders: combobox suggestions, network connect and login, scan phases, license activation, AI status, viewer search, and debug index scans.'
    const MD_USAGE =
        'Mid-size loaders: network mount, SMB reconnect and share enumeration, MTP connect, and command-palette and search results.'
    const LG_USAGE = 'Full-area loaders: initial pane load, and image and PDF viewer load.'
</script>

<SectionCard id="graphics-spinners" label="Spinners">
    <p class="intro">
        The one loading spinner (<code>Spinner</code>), sized via the <code>size</code> prop. The accent-colored ring
        freezes static under <code>prefers-reduced-motion</code>.
    </p>
    <div class="grid">
        <div class="cell" use:tooltip={SM_USAGE}>
            <div class="spinner-host">
                <Spinner size="sm" />
            </div>
            <p class="caption">sm (12px)</p>
        </div>
        <div class="cell" use:tooltip={MD_USAGE}>
            <div class="spinner-host">
                <Spinner size="md" />
            </div>
            <p class="caption">md (24px)</p>
        </div>
        <div class="cell" use:tooltip={LG_USAGE}>
            <div class="spinner-host">
                <Spinner size="lg" />
            </div>
            <p class="caption">lg (32px)</p>
        </div>
    </div>
</SectionCard>

<style>
    .intro {
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .intro code {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
    }

    .grid {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
        gap: var(--spacing-lg);
    }

    .cell {
        display: flex;
        flex-direction: column;
        align-items: center;
    }

    .spinner-host {
        height: 48px;
        display: flex;
        align-items: center;
    }

    .caption {
        margin: var(--spacing-sm) 0 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        text-align: center;
    }
</style>
