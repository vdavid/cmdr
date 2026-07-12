<script lang="ts">
    import { addToast, dismissTransientToasts, clearAllToasts, getToasts } from '$lib/ui/toast'
    import { pluralize } from '$lib/utils/pluralize'

    let toastCounter = $state(0)
</script>

<section class="debug-section">
    <h2>Toast notifications</h2>
    <div class="toast-debug-panel">
        <div class="toast-debug-row">
            <span class="toast-debug-label">Transient</span>
            <button
                class="index-button"
                onclick={() => {
                    toastCounter++
                    addToast(`Default toast #${String(toastCounter)}`)
                }}>Default</button
            >
            <button
                class="index-button"
                onclick={() => {
                    toastCounter++
                    addToast(`Info toast #${String(toastCounter)}`, { level: 'info' })
                }}>Info</button
            >
            <button
                class="index-button"
                onclick={() => {
                    toastCounter++
                    addToast(`Success toast #${String(toastCounter)}`, { level: 'success' })
                }}>Success</button
            >
            <button
                class="index-button"
                onclick={() => {
                    toastCounter++
                    addToast(`Warning toast #${String(toastCounter)}`, { level: 'warn' })
                }}>Warn</button
            >
            <button
                class="index-button"
                onclick={() => {
                    toastCounter++
                    addToast(`Error toast #${String(toastCounter)}`, { level: 'error' })
                }}>Error</button
            >
        </div>
        <div class="toast-debug-row">
            <span class="toast-debug-label">Persistent</span>
            <button
                class="index-button"
                onclick={() => {
                    toastCounter++
                    addToast(`Persistent default #${String(toastCounter)}`, { dismissal: 'persistent' })
                }}>Default</button
            >
            <button
                class="index-button"
                onclick={() => {
                    toastCounter++
                    addToast(`Persistent info #${String(toastCounter)}`, {
                        dismissal: 'persistent',
                        level: 'info',
                    })
                }}>Info</button
            >
            <button
                class="index-button"
                onclick={() => {
                    toastCounter++
                    addToast(`Persistent success #${String(toastCounter)}`, {
                        dismissal: 'persistent',
                        level: 'success',
                    })
                }}>Success</button
            >
            <button
                class="index-button"
                onclick={() => {
                    toastCounter++
                    addToast(`Persistent warning #${String(toastCounter)}`, {
                        dismissal: 'persistent',
                        level: 'warn',
                    })
                }}>Warn</button
            >
            <button
                class="index-button"
                onclick={() => {
                    toastCounter++
                    addToast(`Persistent error #${String(toastCounter)}`, {
                        dismissal: 'persistent',
                        level: 'error',
                    })
                }}>Error</button
            >
        </div>
        <div class="toast-debug-row">
            <span class="toast-debug-label">Dedup</span>
            <button
                class="index-button"
                onclick={() => {
                    toastCounter++
                    addToast(`Dedup toast (always replaces) #${String(toastCounter)}`, { id: 'dedup-test' })
                }}>Replace (same ID)</button
            >
        </div>
        <div class="toast-debug-row">
            <span class="toast-debug-label">Custom timeout</span>
            <button
                class="index-button"
                onclick={() => {
                    toastCounter++
                    addToast(`10s timeout #${String(toastCounter)}`, { timeoutMs: 10000 })
                }}>10 seconds</button
            >
        </div>
        <div class="toast-debug-row">
            <span class="toast-debug-label">Bulk actions</span>
            <button class="index-button" onclick={dismissTransientToasts}>Dismiss transient</button>
            <button class="index-button" onclick={clearAllToasts}>Clear all</button>
        </div>
        <div class="toast-debug-row">
            <span class="toast-debug-label">Active</span>
            <span class="toast-debug-count">{getToasts().length} {pluralize(getToasts().length, 'toast')}</span>
        </div>
    </div>
</section>

<style>
    .toast-debug-panel {
        background: var(--color-bg-secondary);
        border-radius: var(--radius-md);
        padding: var(--spacing-md);
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
    }

    .toast-debug-row {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    .toast-debug-label {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        min-width: 110px;
    }

    .toast-debug-count {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        font-family: var(--font-mono);
    }
</style>
