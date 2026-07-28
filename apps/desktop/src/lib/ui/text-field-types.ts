/**
 * Shared prop types for the text-field primitives (`TextInput.svelte`, `TextArea.svelte`).
 *
 * They live in a plain `.ts` module, NOT in a component's `<script module>`, for the same reason
 * `menu-types.ts` does: a type imported from a `.svelte` file resolves to `any` under the
 * plain-TypeScript lint service, so `TextArea` importing them from `TextInput.svelte` would make
 * its own props untyped (and trip `@typescript-eslint/no-unsafe-assignment`).
 */

/**
 * Text-ish input kinds. `number` is here only for a numeric field embedded in another control (the
 * settings `Select`'s inline "Custom…" value), where `min` / `max` semantics matter: a STANDALONE
 * numeric setting uses `NumberInput`, the house number control. Anything else (checkbox, radio,
 * color) has its own primitive.
 */
export type TextInputType = 'text' | 'password' | 'email' | 'search' | 'url' | 'tel' | 'number'

/**
 * Corner rounding. `lg` is the house text field (~25% of the field height, the macOS look); `full`
 * is the search-pill shape.
 */
export type TextFieldRadius = 'sm' | 'md' | 'lg' | 'full'

/**
 * `default` draws the framed field. `chromeless` drops the frame (border, background, padding,
 * focus ring) for an inline / overlay field whose host already draws a surface, and inherits the
 * host's type scale, while keeping the caret, selection, and placeholder contract.
 */
export type TextFieldVariant = 'default' | 'chromeless'
