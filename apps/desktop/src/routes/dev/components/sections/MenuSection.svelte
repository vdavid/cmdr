<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Menu, { type MenuItem } from '$lib/ui/Menu.svelte'
    import DemoAnchor from '../DemoAnchor.svelte'

    let open = $state(false)
    let anchorPoint = $state<{ x: number; y: number } | null>(null)
    let lastChoice = $state<string | null>(null)

    const items: MenuItem[] = [
        { value: 'browse', label: 'Browse like a folder' },
        { value: 'open', label: 'Open with default app' },
        { value: 'configure', label: 'Configure…' },
    ]

    let highlighted = $state<string | null>('browse')

    function openAt(event: MouseEvent): void {
        anchorPoint = { x: event.clientX, y: event.clientY }
        highlighted = 'browse'
        open = true
    }

    function handleSelect(value: string): void {
        lastChoice = value
        open = false
    }

    function handleHighlight(value: string | null): void {
        highlighted = value
    }
</script>

<SectionCard id="components-menu" label="Menu">
    <div class="cell">
        <p class="caption">
            Presentational action menu: rendered at a point, mounted only while open. Click the anchor to open it at the
            cursor; click a row or outside to dismiss.
        </p>
        <DemoAnchor onclick={openAt}>Open menu here</DemoAnchor>
        {#if lastChoice}
            <p class="caption">Last choice: {lastChoice}</p>
        {/if}
        {#if open}
            <Menu
                onSelect={handleSelect}
                onClose={() => {
                    open = false
                }}
                {items}
                {anchorPoint}
                highlightedValue={highlighted}
                onHighlightChange={handleHighlight}
                ariaLabel="Demo menu"
            />
        {/if}
    </div>
</SectionCard>

<style>
    .caption {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }
</style>
