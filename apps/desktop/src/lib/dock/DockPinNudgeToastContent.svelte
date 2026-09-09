<script lang="ts">
    /**
     * The once-ever INFO toast that asks whether Cmdr may sit in the Dock, raised
     * a few days into using it.
     *
     * `dock-nudge.ts` raises it and stamps the nudge ledger, so this component
     * only has to carry the copy and the two answers. The frame's own X is the
     * third answer and is recorded at the raise site, since this component never
     * sees it. The markup is `$lib/nudges/NudgeToastContent.svelte`, shared with
     * the "Show in Finder" offer.
     */
    import NudgeToastContent from '$lib/nudges/NudgeToastContent.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { acceptDockPin, declineDockPin } from './dock-pin-answer'

    interface Props {
        /** Dedup id of this toast, so either answer can retire it. */
        toastId: string
    }

    const { toastId }: Props = $props()
</script>

<NudgeToastContent
    title={tString('main.dockPinNudge.title')}
    body={tString('main.dockPinNudge.body')}
    note={tString('main.dockPinNudge.unpinNote')}
    declineLabel={tString('main.dockPinNudge.decline')}
    acceptLabel={tString('main.dockPinNudge.accept')}
    onDecline={() => {
        declineDockPin(toastId)
    }}
    onAccept={() => {
        void acceptDockPin(toastId)
    }}
/>
