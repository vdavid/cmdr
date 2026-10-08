/**
 * Tail follow: with tail mode on, a viewport parked at the end of the file stays there as the
 * file grows, the way `tail -f` reads. A viewport the user scrolled up stays where they put it.
 */

export interface ScrollMetrics {
  scrollTop: number
  scrollHeight: number
  clientHeight: number
}

/**
 * Whether the viewport shows the end of the content. `slack` (one row's height) absorbs the
 * fractional gap that zoom and scaled row heights leave at the bottom.
 */
export function isScrolledToEnd({ scrollTop, scrollHeight, clientHeight }: ScrollMetrics, slack: number): boolean {
  return scrollHeight - clientHeight - scrollTop <= slack
}
