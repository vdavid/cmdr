<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { addToast } from '$lib/ui/toast'
    import type { ToastLevel } from '$lib/ui/toast'
    import ToastItem from '$lib/ui/toast/ToastItem.svelte'

    interface Preview {
        level: ToastLevel
        label: string
        message: string
        /** How long ago the preview pretends it was posted, so some of them show the age label. */
        ageMs: number
    }

    const MINUTE_MS = 60_000

    const previews: Preview[] = [
        { level: 'default', label: 'default', message: 'Connecting directly...', ageMs: 0 },
        { level: 'info', label: 'info', message: 'Copied 12 items.', ageMs: 0 },
        { level: 'success', label: 'success', message: 'Share disconnected.', ageMs: 5 * MINUTE_MS },
        {
            level: 'warn',
            label: 'warn',
            message: 'Tab limit reached. Close a tab in this pane to open a new one.',
            ageMs: 2 * MINUTE_MS,
        },
        { level: 'error', label: 'error', message: "Couldn't remove host.", ageMs: 65 * MINUTE_MS },
    ]

    const renderedAt = Date.now()

    function ignore(): void {
        // The previews are static: nothing times out, and dismissing one does nothing.
    }

    function triggerToast(level: ToastLevel) {
        const preview = previews.find((p) => p.level === level)
        addToast(preview?.message ?? `${level} toast`, { level })
    }

    function triggerPersistent() {
        addToast('Persistent toast (catalog preview).', { level: 'info', dismissal: 'persistent' })
    }

    // Group + hover demos.

    function triggerGroupBurst() {
        // Fires 6 toasts in the 'demo' group with cap 5; the first one
        // should evict instantly to demonstrate FIFO-in-group eviction.
        for (let i = 1; i <= 6; i++) {
            addToast(`Demo notification ${String(i)} of 6`, {
                level: 'info',
                toastGroup: 'demo',
            })
        }
    }

    function triggerHoverDemo() {
        addToast('Keep hovering past six seconds, then move away: I hide a second later.', {
            level: 'info',
            timeoutMs: 6000,
        })
    }
</script>

<SectionCard id="components-toasts" label="Toasts">
    <p class="caption">
        The real toast frame at each level (default, info, success, warn, error), backdated so the older ones show
        their age label.
    </p>
    <div class="preview-stack">
        {#each previews as p (p.level)}
            <ToastItem
                id={`preview-${p.level}`}
                content={p.message}
                level={p.level}
                dismissal="persistent"
                timeoutMs={0}
                postedAt={renderedAt - p.ageMs}
                onTimeout={ignore}
                onUserDismiss={ignore}
            />
        {/each}
    </div>

    <p class="caption">Trigger a real toast:</p>
    <div class="trigger-row">
        {#each previews as p (p.level)}
            <Button
                size="mini"
                onclick={() => {
                    triggerToast(p.level)
                }}
            >
                {p.label}
            </Button>
        {/each}
        <Button size="mini" onclick={triggerPersistent}>persistent</Button>
    </div>

    <p class="caption">
        Group cap: fire 6 toasts of group <code>'demo'</code> with cap 5; the oldest in the group is evicted instantly.
    </p>
    <div class="trigger-row">
        <Button size="mini" onclick={triggerGroupBurst}>Burst of 6 grouped toasts</Button>
    </div>

    <p class="caption">
        Hover behavior: hovering never pauses a transient toast's clock, but a toast never hides under the pointer.
        Past its natural deadline, it hides one second after the pointer leaves.
    </p>
    <div class="trigger-row">
        <Button size="mini" onclick={triggerHoverDemo}>Show a 6-second toast</Button>
        <span class="hint">Hover the toast top-right; move away to see it keep or lose its remaining time.</span>
    </div>
</SectionCard>

<style>
    .caption {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .hint {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        align-self: center;
    }

    .preview-stack {
        display: flex;
        flex-direction: column;
        align-items: flex-start;
        gap: var(--spacing-md);
        margin-bottom: var(--spacing-lg);
    }

    .trigger-row {
        display: flex;
        flex-wrap: wrap;
        gap: var(--spacing-sm);
        margin-bottom: var(--spacing-md);
    }
</style>
