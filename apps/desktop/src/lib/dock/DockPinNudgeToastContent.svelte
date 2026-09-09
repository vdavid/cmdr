<script lang="ts">
    /**
     * The once-ever INFO toast that asks whether Cmdr may sit in the Dock, raised
     * a few days into using it.
     *
     * `dock-nudge.ts` raises it and spends `behavior.dockPinNudgeSeen`, so this
     * component only has to carry the two answers. The frame's own X is the third
     * answer and is recorded at the raise site, since this component never sees it.
     */
    import Button from '$lib/ui/Button.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { acceptDockPin, declineDockPin } from './dock-pin-answer'

    interface Props {
        /** Dedup id of this toast, so either answer can retire it. */
        toastId: string
    }

    const { toastId }: Props = $props()
</script>

<div class="content">
    <strong class="title">{tString('main.dockPinNudge.title')}</strong>
    <span class="body">{tString('main.dockPinNudge.body')}</span>
    <span class="body">{tString('main.dockPinNudge.unpinNote')}</span>
    <div class="actions">
        <Button
            size="mini"
            variant="secondary"
            onclick={() => {
                declineDockPin(toastId)
            }}>{tString('main.dockPinNudge.decline')}</Button
        >
        <Button
            size="mini"
            variant="primary"
            onclick={() => {
                void acceptDockPin(toastId)
            }}>{tString('main.dockPinNudge.accept')}</Button
        >
    </div>
</div>

<style>
    .content {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
    }

    .title {
        color: var(--color-text-primary);
        font-weight: 600;
    }

    .body {
        color: var(--color-text-primary);
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-sm);
    }
</style>
