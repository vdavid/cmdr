<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Chip from '$lib/ui/Chip.svelte'

    let sizeOpen = $state(false)
    let sizeConfigured = $state(true)
</script>

<SectionCard id="components-chip" label="Chip">
    <div class="grid">
        <div class="cell">
            <p class="caption">Filter variant &mdash; default, configured (with × clear), open, disabled</p>
            <div class="row">
                <Chip label="Modified" configured={false} isOpen={false} onActivate={() => {}} />
                <Chip
                    label="Size"
                    value="> 100 MB"
                    configured={sizeConfigured}
                    isOpen={sizeOpen}
                    onActivate={() => {
                        sizeOpen = !sizeOpen
                    }}
                    onClear={() => {
                        sizeConfigured = false
                    }}
                />
                <Chip label="Search in" configured={false} isOpen={true} onActivate={() => {}} />
                <Chip label="Pattern" configured={false} isOpen={false} disabled onActivate={() => {}} />
            </div>
        </div>

        <div class="cell">
            <p class="caption">Filter variant &mdash; highlighted (AI just populated it)</p>
            <div class="row">
                <Chip label="Size" value="≥ 1 MB" configured={true} isOpen={false} highlighted onActivate={() => {}} onClear={() => {}} />
            </div>
        </div>

        <div class="cell">
            <p class="caption">Recent variant &mdash; mode badge + truncating label</p>
            <div class="row">
                <Chip variant="recent" label="*.jpg" onActivate={() => {}} onContextMenu={() => {}}>
                    {#snippet leading()}<span class="demo-badge">Aa</span>{/snippet}
                </Chip>
                <Chip variant="recent" label="all my very long screenshot file names from last week" onActivate={() => {}}>
                    {#snippet leading()}<span class="demo-badge">AI</span>{/snippet}
                </Chip>
                <Chip variant="recent" label="^temp.*\.log$" onActivate={() => {}}>
                    {#snippet leading()}<span class="demo-badge">.*</span>{/snippet}
                </Chip>
                <Chip variant="recent" label="*.bak" disabled onActivate={() => {}}>
                    {#snippet leading()}<span class="demo-badge">Aa</span>{/snippet}
                </Chip>
            </div>
        </div>
    </div>
</SectionCard>

<style>
    .grid {
        display: grid;
        grid-template-columns: 1fr;
        gap: var(--spacing-lg);
    }

    .caption {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .row {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: var(--spacing-sm);
    }

    .demo-badge {
        font-size: var(--font-size-xs);
        font-family: var(--font-mono);
        font-weight: 600;
        letter-spacing: 0.04em;
        padding: var(--spacing-xxs) var(--spacing-xs);
        background: var(--color-accent-subtle);
        color: var(--color-text-primary);
        border-radius: var(--radius-xs);
        line-height: 1;
    }
</style>
