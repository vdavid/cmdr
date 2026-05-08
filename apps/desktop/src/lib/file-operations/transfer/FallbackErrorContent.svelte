<script lang="ts">
    import type { WriteOperationError, TransferOperationType } from '$lib/file-explorer/types'
    import { getUserFriendlyMessage } from './transfer-error-messages'

    interface Props {
        error: WriteOperationError
        operationType: TransferOperationType
    }

    const { error, operationType }: Props = $props()
    const friendly = $derived(getUserFriendlyMessage(error, operationType))
</script>

<div class="error-content">
    <p id="error-dialog-message" class="message selectable">{friendly.message}</p>
    <p class="suggestion">{friendly.suggestion}</p>
</div>

<style>
    .error-content {
        padding: 0 var(--spacing-xl) var(--spacing-lg);
    }

    .message {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .suggestion {
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        line-height: 1.5;
    }

    .selectable {
        user-select: text;
        -webkit-user-select: text;
        cursor: text;
    }
</style>
