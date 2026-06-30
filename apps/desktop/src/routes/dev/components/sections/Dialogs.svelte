<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Button from '$lib/ui/Button.svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import AlertDialog from '$lib/ui/AlertDialog.svelte'

    let modalOpen = $state(false)
    let modalBlurOpen = $state(false)
    let alertOpen = $state(false)
</script>

<SectionCard id="components-dialogs" label="Dialogs">
    <div class="rows">
        <div>
            <p class="example-caption">Modal dialog (default and blurred overlay)</p>
            <div class="preview-row">
                <div class="dialog-preview" role="presentation">
                    <div class="dp-titlebar">
                        <h4 class="dp-title">Confirm rename</h4>
                    </div>
                    <div class="dp-body">
                        <p class="dp-message">Rename "draft.md" to "final.md"?</p>
                        <div class="dp-button-row">
                            <span class="dp-stub-btn dp-stub-secondary">Cancel</span>
                            <span class="dp-stub-btn dp-stub-primary">Rename</span>
                        </div>
                    </div>
                </div>

                <div class="dialog-preview" role="presentation">
                    <div class="dp-titlebar">
                        <h4 class="dp-title">Confirm rename</h4>
                    </div>
                    <div class="dp-body">
                        <p class="dp-message">With blur overlay (real overlay is portal-mounted).</p>
                        <div class="dp-button-row">
                            <span class="dp-stub-btn dp-stub-secondary">Cancel</span>
                            <span class="dp-stub-btn dp-stub-primary">Rename</span>
                        </div>
                    </div>
                </div>
            </div>
            <div class="trigger-row">
                <Button
                    onclick={() => {
                        modalOpen = true
                    }}
                >
                    Trigger modal dialog
                </Button>
                <Button
                    onclick={() => {
                        modalBlurOpen = true
                    }}
                >
                    Trigger modal dialog with blur
                </Button>
            </div>
        </div>

        <div>
            <p class="example-caption">Alert dialog (single action)</p>
            <div class="preview-row">
                <div class="dialog-preview dp-alert" role="presentation">
                    <div class="dp-titlebar">
                        <h4 class="dp-title">Couldn't save settings</h4>
                    </div>
                    <div class="dp-body">
                        <p class="dp-message">The settings file is read-only. Check folder permissions.</p>
                        <div class="dp-button-row">
                            <span class="dp-stub-btn dp-stub-primary">OK</span>
                        </div>
                    </div>
                </div>
            </div>
            <div class="trigger-row">
                <Button
                    onclick={() => {
                        alertOpen = true
                    }}
                >
                    Trigger alert dialog
                </Button>
            </div>
        </div>
    </div>
</SectionCard>

{#if modalOpen}
    <ModalDialog
        titleId="catalog-modal-title"
        onclose={() => {
            modalOpen = false
        }}
        containerStyle="width: 360px"
    >
        {#snippet title()}Modal dialog preview{/snippet}
        <div class="real-body">
            <p>This is the real ModalDialog mounted on demand.</p>
        </div>
        {#snippet footer()}
            <Button
                variant="primary"
                onclick={() => {
                    modalOpen = false
                }}
            >
                Close
            </Button>
        {/snippet}
    </ModalDialog>
{/if}

{#if modalBlurOpen}
    <ModalDialog
        titleId="catalog-modal-blur-title"
        blur
        onclose={() => {
            modalBlurOpen = false
        }}
        containerStyle="width: 360px"
    >
        {#snippet title()}Modal dialog with blur{/snippet}
        <div class="real-body">
            <p>Backdrop uses `backdrop-filter: blur(4px)`.</p>
        </div>
        {#snippet footer()}
            <Button
                variant="primary"
                onclick={() => {
                    modalBlurOpen = false
                }}
            >
                Close
            </Button>
        {/snippet}
    </ModalDialog>
{/if}

{#if alertOpen}
    <AlertDialog
        title="Catalog preview"
        message="This is the real AlertDialog, mounted on demand."
        onClose={() => {
            alertOpen = false
        }}
    />
{/if}

<style>
    .rows {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xl);
    }

    .example-caption {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .preview-row {
        display: flex;
        flex-wrap: wrap;
        gap: var(--spacing-lg);
        margin-bottom: var(--spacing-md);
    }

    .trigger-row {
        display: flex;
        gap: var(--spacing-sm);
        flex-wrap: wrap;
    }

    /* Static preview matching ModalDialog visuals. */
    .dialog-preview {
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-lg);
        box-shadow: var(--shadow-lg);
        width: 280px;
    }

    .dp-alert {
        width: 240px;
    }

    .dp-titlebar {
        padding: var(--spacing-xl) var(--spacing-xl) var(--spacing-md);
    }

    .dp-title {
        margin: 0;
        font-size: var(--font-size-lg);
        font-weight: 600;
        color: var(--color-text-primary);
        text-align: left;
    }

    .dp-body {
        padding: 0 var(--spacing-xl) var(--spacing-xl);
    }

    .dp-message {
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .dp-button-row {
        display: flex;
        gap: var(--spacing-sm);
        justify-content: flex-end;
    }

    /* Decorative button stubs inside the static preview (not real buttons,
       so they don't trip the btn-restyle guard). */
    .dp-stub-btn {
        display: inline-block;
        font-size: var(--font-size-md);
        font-weight: 500;
        line-height: 1.5;
        padding: 7px 20px;
        border-radius: var(--radius-md);
    }

    .dp-stub-primary {
        background: var(--color-accent);
        color: var(--color-accent-fg);
    }

    .dp-stub-secondary {
        background: transparent;
        color: var(--color-text-secondary);
        border: 1px solid var(--color-border);
    }

    .real-body {
        padding: 0 var(--spacing-xl);
    }

    .real-body p {
        margin: 0;
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
    }
</style>
