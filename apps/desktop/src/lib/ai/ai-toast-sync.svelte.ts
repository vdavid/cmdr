import AiToastContent from './AiToastContent.svelte'
import { getAiState } from './ai-state.svelte'
import { addToast, dismissToast } from '$lib/ui/toast'

export function initAiToastSync(): void {
  $effect(() => {
    if (getAiState().notificationState === 'hidden') {
      dismissToast('ai')
    } else {
      addToast(AiToastContent, { id: 'ai', dismissal: 'persistent' })
    }
  })
}
