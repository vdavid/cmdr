<script lang="ts">
    /**
     * The once-ever INFO toast offering "Open in Cmdr" at the top level of Finder's
     * right-click menu. `finder-extension-nudge.ts` raises it and stamps the ledger, so
     * this only carries the copy and the two answers. The frame's × is the third answer,
     * recorded at the raise site. The markup is `#lib/nudges/NudgeToastContent.svelte`.
     */
    import NudgeToastContent from '#lib/nudges/NudgeToastContent.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { acceptFinderExtensionNudge, declineFinderExtensionNudge } from './finder-extension-nudge-answer'

    interface Props {
        /** Dedup id of this toast, so either answer can retire it. */
        toastId: string
    }

    const { toastId }: Props = $props()
</script>

<NudgeToastContent
    title={tString('main.finderExtensionNudge.title')}
    body={tString('main.finderExtensionNudge.body')}
    note={tString('main.finderExtensionNudge.switchBackNote')}
    declineLabel={tString('main.finderExtensionNudge.decline')}
    acceptLabel={tString('main.finderExtensionNudge.accept')}
    onDecline={() => {
        declineFinderExtensionNudge(toastId)
    }}
    onAccept={() => {
        void acceptFinderExtensionNudge(toastId)
    }}
/>
