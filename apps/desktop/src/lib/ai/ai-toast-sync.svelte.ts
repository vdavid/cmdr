import AiToastContent from './AiToastContent.svelte'
import { getAiState, markDownloadToastDismissed } from './ai-state.svelte'
import { addToast, dismissToast } from '$lib/ui/toast'

export function initAiToastSync(): void {
  $effect(() => {
    const state = getAiState()
    if (state.notificationState === 'hidden') {
      dismissToast('ai')
      return
    }

    if (state.notificationState === 'downloading') {
      // Once the user closes the downloading toast with X, keep it dismissed for the rest of this
      // run. The download itself keeps going: only the toast is hidden. The flag resets on the
      // next download run (kicked off by the wizard's step 2 when the user picks the local
      // provider) and other state transitions still surface fresh.
      if (state.downloadToastUserDismissed) {
        return
      }
      addToast(AiToastContent, {
        id: 'ai',
        dismissal: 'persistent',
        closeTooltip: 'Close this notification — the download will continue in the background',
        onDismiss: markDownloadToastDismissed,
      })
      return
    }

    addToast(AiToastContent, { id: 'ai', dismissal: 'persistent' })
  })
}
