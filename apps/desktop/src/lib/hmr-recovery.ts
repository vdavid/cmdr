/**
 * SvelteKit's client router crashes with "Cannot access 'component' before
 * initialization" when HMR updates propagate through the root layout (for
 * example, `app.css` changes). This handler catches the crash and forces a
 * clean page reload. Dev-mode only. See sveltejs/kit#15287.
 *
 * Must be imported from a stable module (not the root layout itself) so the
 * listener survives layout component re-evaluation during HMR.
 */
if (import.meta.hot) {
  const DEBOUNCE_KEY = '__hmr_last_reload'
  const DEBOUNCE_MS = 3000

  window.addEventListener('unhandledrejection', (event) => {
    // eslint-disable-next-line cmdr/no-error-string-match -- workaround for sveltejs/kit#15287; remove when upstream fix lands
    if (event.reason instanceof ReferenceError && event.reason.message.includes('component')) {
      event.preventDefault()

      // Debounce via sessionStorage (survives page reloads, unlike JS variables
      // which reset when HMR invalidates the module)
      const now = Date.now()
      const lastReload = Number(sessionStorage.getItem(DEBOUNCE_KEY) ?? '0')
      if (now - lastReload < DEBOUNCE_MS) {
        // eslint-disable-next-line no-console -- dev-only HMR crash recovery; app logger may not be initialized yet at this point
        console.warn('[HMR] SvelteKit TDZ crash detected, skipping reload (debounce)')
        return
      }

      sessionStorage.setItem(DEBOUNCE_KEY, String(now))
      // eslint-disable-next-line no-console -- dev-only HMR crash recovery; app logger may not be initialized yet at this point
      console.warn('[HMR] SvelteKit component TDZ crash detected, reloading page')
      location.reload()
    }
  })
}
