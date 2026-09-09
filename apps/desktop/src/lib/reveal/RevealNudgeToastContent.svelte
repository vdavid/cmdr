<script lang="ts">
    /**
     * The once-ever INFO toast that asks whether another app's "Show in Finder"
     * may land in Cmdr, raised a couple of days into using it.
     *
     * `reveal-nudge.ts` raises it and stamps the nudge ledger, so this component
     * only has to carry the copy and the two answers. The frame's own × is the
     * third answer and is recorded at the raise site, since this component never
     * sees it. The markup is `$lib/nudges/NudgeToastContent.svelte`, shared with
     * the Dock offer.
     */
    import NudgeToastContent from '$lib/nudges/NudgeToastContent.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { acceptRevealNudge, declineRevealNudge } from './reveal-nudge-answer'

    interface Props {
        /** Dedup id of this toast, so either answer can retire it. */
        toastId: string
    }

    const { toastId }: Props = $props()
</script>

<NudgeToastContent
    title={tString('main.revealNudge.title')}
    body={tString('main.revealNudge.body')}
    note={tString('main.revealNudge.switchBackNote')}
    declineLabel={tString('main.revealNudge.decline')}
    acceptLabel={tString('main.revealNudge.accept')}
    onDecline={() => {
        declineRevealNudge(toastId)
    }}
    onAccept={() => {
        void acceptRevealNudge(toastId)
    }}
/>
