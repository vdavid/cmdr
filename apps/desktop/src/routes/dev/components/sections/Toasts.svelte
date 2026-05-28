<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { addToast } from '$lib/ui/toast'
    import type { ToastLevel } from '$lib/ui/toast'

    interface Preview {
        level: ToastLevel
        label: string
        message: string
    }

    const previews: Preview[] = [
        { level: 'default', label: 'default', message: 'Connecting directly...' },
        { level: 'info', label: 'info', message: 'Copied 12 items.' },
        { level: 'success', label: 'success', message: 'Share disconnected.' },
        { level: 'warn', label: 'warn', message: 'Tab limit reached.' },
        { level: 'error', label: 'error', message: "Couldn't remove host." },
    ]

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
        addToast('Hover me to pause; leaving past expiry gives a 2-second grace.', {
            level: 'info',
            timeoutMs: 6000,
        })
    }
</script>

<SectionCard id="components-toasts" label="Toasts">
    <p class="caption">Static previews of each level (left to right: default, info, success, warn, error).</p>
    <div class="preview-row">
        {#each previews as p (p.level)}
            <div
                class="toast-preview"
                class:info={p.level === 'info'}
                class:success={p.level === 'success'}
                class:warn={p.level === 'warn'}
                class:error={p.level === 'error'}
            >
                <span class="toast-message">{p.message}</span>
                <span class="toast-close" aria-hidden="true">×</span>
            </div>
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
        Hover behavior: the timer pauses while the pointer is over a transient toast. Past natural expiry, leaving
        starts a 2-second grace timer.
    </p>
    <div class="trigger-row">
        <Button size="mini" onclick={triggerHoverDemo}>Show a hover-pause toast</Button>
        <span class="hint">Hover the toast top-right; move away to see the resume or grace behavior.</span>
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

    .preview-row {
        display: flex;
        flex-wrap: wrap;
        gap: var(--spacing-sm);
        margin-bottom: var(--spacing-lg);
    }

    .trigger-row {
        display: flex;
        flex-wrap: wrap;
        gap: var(--spacing-sm);
        margin-bottom: var(--spacing-md);
    }

    /* Mirror ToastItem.svelte chrome for static previews. */
    .toast-preview {
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border-subtle);
        border-left: 3px solid var(--color-text-tertiary);
        border-radius: var(--radius-md);
        box-shadow: var(--shadow-md);
        padding: var(--spacing-md) var(--spacing-lg);
        font-size: var(--font-size-sm);
        max-width: 240px;
        display: flex;
        align-items: start;
        gap: var(--spacing-sm);
    }

    .toast-preview.info {
        border-left-color: var(--color-toast-info-stripe);
        background: var(--color-toast-info-bg);
    }

    .toast-preview.success {
        border-left-color: var(--color-toast-success-stripe);
        background: var(--color-toast-success-bg);
    }

    .toast-preview.warn {
        border-left-color: var(--color-toast-warn-stripe);
        background: var(--color-toast-warn-bg);
    }

    .toast-preview.error {
        border-left-color: var(--color-error);
        background: var(--color-toast-error-bg);
    }

    .toast-message {
        flex: 1;
        color: var(--color-text-primary);
        line-height: 1.4;
    }

    .toast-close {
        flex-shrink: 0;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        line-height: 1;
    }
</style>
