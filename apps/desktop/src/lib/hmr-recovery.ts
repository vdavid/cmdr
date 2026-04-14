/**
 * SvelteKit's client router crashes with "Cannot access 'component' before
 * initialization" when HMR updates propagate through the root layout
 * (virtual:uno.css, app.css changes). This handler catches the crash and
 * forces a clean page reload. Dev-mode only.
 *
 * Must be imported from a stable module (not the root layout itself) so the
 * listener survives layout component re-evaluation during HMR.
 */
if (import.meta.hot) {
  const DEBOUNCE_KEY = '__hmr_last_reload'
  const DEBOUNCE_MS = 3000

  window.addEventListener('unhandledrejection', (event) => {
    if (event.reason instanceof ReferenceError && event.reason.message.includes('component')) {
      event.preventDefault()

      // Debounce via sessionStorage (survives page reloads, unlike JS variables
      // which reset when HMR invalidates the module)
      const now = Date.now()
      const lastReload = Number(sessionStorage.getItem(DEBOUNCE_KEY) ?? '0')
      if (now - lastReload < DEBOUNCE_MS) {
        console.warn('[HMR] SvelteKit TDZ crash detected, skipping reload (debounce)')
        return
      }

      sessionStorage.setItem(DEBOUNCE_KEY, String(now))
      console.warn('[HMR] SvelteKit component TDZ crash detected, reloading page')
      location.reload()
    }
  })
}
