<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
</script>

<SectionCard id="components-shortcut-chip" label="Shortcut chip">
    <div class="rows">
        <div class="row">
            <span class="row-label">Literal keys</span>
            <div class="chips">
                <ShortcutChip key="⏎" />
                <ShortcutChip key="⇧Space" />
                <ShortcutChip key="⌘⇧⌥K" />
            </div>
        </div>

        <div class="row">
            <span class="row-label">Command (bound, clickable)</span>
            <div class="chips">
                <ShortcutChip commandId="downloads.goToLatest" />
            </div>
            <span class="row-note">Hover for accent border; click opens Settings &gt; Keyboard shortcuts</span>
        </div>

        <div class="row">
            <span class="row-label">Command (bound, non-clickable)</span>
            <div class="chips">
                <ShortcutChip commandId="downloads.goToLatest" clickable={false} />
            </div>
            <span class="row-note">For chips nested inside another interactive control (palette rows, F-key bar)</span>
        </div>

        <div class="row">
            <span class="row-label">Dense (size="sm")</span>
            <div class="chips">
                <ShortcutChip key="⌘1" size="sm" />
                <ShortcutChip key="⌘2" size="sm" />
                <ShortcutChip key="⌘3" size="sm" />
            </div>
            <span class="row-note">Tighter padding + radius for dense rows (the command palette)</span>
        </div>

        <div class="row">
            <span class="row-label">Command (unbound)</span>
            <div class="chips">
                <ShortcutChip commandId="app.about" />
                <span class="row-note">Renders nothing (the command has no binding)</span>
            </div>
        </div>
    </div>
</SectionCard>

<style>
    .rows {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-md);
    }

    .row {
        display: flex;
        gap: var(--spacing-lg);
        align-items: center;
        flex-wrap: wrap;
    }

    .row-label {
        min-width: 200px;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .chips {
        display: flex;
        gap: var(--spacing-md);
        align-items: center;
        flex-wrap: wrap;
    }

    .row-note {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }
</style>
