<script lang="ts">
    /** Shows whichever question a tool is waiting on. Mounted unconditionally by the main page. */
    import ToolAskDialog from './ToolAskDialog.svelte'
    import { allowToolAsk, declineToolAsk, getToolAskQuestion } from './tool-ask.svelte'

    const question = $derived(getToolAskQuestion())
</script>

{#if question}
    {#key question.requestId}
        <ToolAskDialog
            facts={question.facts}
            startedBy={question.startedBy}
            onAllow={allowToolAsk}
            onDecline={declineToolAsk}
        />
    {/key}
{/if}
