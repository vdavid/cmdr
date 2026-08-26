import { openExternalUrl, openSystemSettingsUrl } from '$lib/tauri-commands'

/**
 * Click delegate for anchors inside `{@html}`-rendered markdown.
 *
 * Tauri blocks raw `<a>` navigation, so a link in rendered markdown is inert
 * without this: it looks clickable and does nothing. The blocks render their
 * HTML in one shot, so there's no per-anchor handler to attach; the container
 * takes the click and this resolves the anchor from the target.
 *
 * `x-apple.systempreferences:` URLs go through a dedicated Rust IPC, because
 * Tauri's opener plugin allows http/https/mailto/tel only and swallows the rest.
 *
 * Every caller renders trusted markdown (our committed `CHANGELOG.md`, or the
 * backend's friendly-error strings), so no URL allowlisting happens here. Feed it
 * user-authored markdown and that stops being true.
 *
 * Attach it as the container's `onclick`, next to the `{@html}` it wraps:
 *
 * ```svelte
 * <!-- svelte-ignore a11y_no_static_element_interactions -->
 * <div class="explanation" onclick={handleMarkdownLinkClick}>{@html rendered}</div>
 * ```
 */
export function handleMarkdownLinkClick(e: MouseEvent): void {
  const link = (e.target instanceof Element ? e.target : null)?.closest('a')
  const href = link?.getAttribute('href')
  if (!link || !href) return
  e.preventDefault()
  if (href.startsWith('x-apple.systempreferences:')) {
    void openSystemSettingsUrl(href)
  } else {
    void openExternalUrl(href)
  }
}
