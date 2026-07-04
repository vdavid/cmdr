import type { IconName } from './icons/icon-map'

/**
 * One row in a `Menu` (`lib/ui/Menu.svelte`). Lives in a `.ts` (not the component's
 * module script like `SelectItem`) because non-Svelte glue — the archive Enter-menu
 * helpers — consumes it, and a type imported from a `.svelte` file resolves to `any`
 * under the plain-TypeScript lint service.
 *
 * `value` is the stable identity emitted on select; `label` is the visible text;
 * `icon` renders a Lucide glyph before the label; `disabled` greys the row and blocks
 * activation.
 */
export interface MenuItem {
  value: string
  label: string
  icon?: IconName
  disabled?: boolean
}
