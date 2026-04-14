# Style guide

Writing, code, and design styles.

## Writing

- Wording
  - **Use a friendly style**: Make all texts informal, friendly, encouraging, and concise.
  - **Use active voice**: Always prefer active voice. "We moved your files" not "Your files were moved." This is
    especially important for success messages, error messages, and UI copy. Passive voice creeps in easily. Watch for
    it.
  - **Abbreviate English**: Use "I'm", "don't", and such.
  - **Don't trivialize**: Avoid terminology of "just", "simple", "easy", and "all you have to do".
  - **Use gender-neutral language**: Use they/them rather than he/him/she/her. Use "folks" or "everyone" rather than
    "guys".
  - **Use universally understood terms**: Use "start" instead of "kickoff", and "end" instead of "wrap up".
  - **Avoid ableist language**: "placeholder value" rather than "dummy value". No "lame", "sanity check" which derive
    from disabilities.
  - **Avoid violent terms**: "stop a process" rather than "kill" or "nuke" it.
  - **Avoid exclusionary terminology**: Prefer "primary/secondary" or "main/replica" over "master/slave". Use
    "allowlist/denylist" over "whitelist/blacklist".
  - **Use verbs, not verb-noun phrases**: "Search" not "Make a search". "Save" not "Perform a save".
  - **Don't use permissive language**: Give users confidence. "Add repos and start searching" not "Add repos and you can
    start searching."
  - **Be mindful of user expertise**: Avoid jargon. Link to definitions and explain concepts when necessary.
  - **Avoid latinisms**: For example, use "for example" instead of "e.g.".
  - **Avoid abbreviations**: Very common acronyms like "URL" are okay.
  - **Some casual terms are okay**: Use "docs", not "documentation". Use "dev" for developer and "gen" for generation
    where appropriate and understandable.
- Punctuation, capitalization, numbers
  - **Use sentence case in titles**: Regardless whether visible on the UI or dev only.
  - **Use sentence case in labels**: Applies to buttons, labels, and similar. But omit periods on short microcopy.
  - **Capitalize names correctly**: For example, there is GitHub but mailcow.
  - **Use the Oxford comma**: Use "1, 2, and 3" rather than "1, 2 and 3".
  - **Use en dashes but no em dashes**: en dash for ranges, but avoid structures that'd need an em dash.
  - **Use colon for lists**: Use the format I used in this list you're reading right now.
  - **Spell out numbers one through nine.** Use numerals for 10+.
  - **Use ISO dates**: Use YYYY-MM-DD wherever it makes sense.
  - **Format large numbers with thousands separators.** Use `formatNumber()` for all user-facing counts (file counts,
    dir counts, item counts). Byte values use `formatBytes()` / `formatFileSize()` which already handle this.
- UI
  - **Error messages**: Keep conversational, positive, actionable, and specific. Never use the words "error" or "failed"
    — we wouldn't say those in conversation. Suggest a next step.
    - "Couldn't rename the file. Try again?" not "Error: Rename operation failed."
    - "Password must contain at least 12 characters" not "Password format is invalid (minimum 12 characters)"
    - "Sorry, we couldn't save your changes. Try again?" not "Failed to save changes."
  - **Success messages**: Talk about the user, not the action. Make success implicit and warm.
    - "Your files moved to ~/Documents" not "Move operation completed successfully."
    - "Shortcut saved — it's ready to use" not "Shortcut successfully created."
  - **Confirmation dialogs**: Title = `verb + noun` question. Body = plain irreversibility warning. Buttons = outcome
    verbs, never "Yes / No".
    - "Delete 3 files?" / "This can't be undone" / **Cancel** · **Delete**
    - "Discard unsaved changes?" / **Cancel** · **Discard**
  - **Empty states**: Say what belongs here and offer a next step. Empty states reveal potential, not absence.
    - "Your bookmarks will appear here. Create your first bookmark!" not "No bookmarks found."
  - **Link the destination, not the sentence**: In sentences, only link the text that describes where you'll go.
    - "Learn how to [set up shortcuts]" not "[Learn how to set up shortcuts]."
  - **Helper text**: Only add if users actually need it. More messages = more intimidating. Keep it short.
    - "8–12 characters" not "Password must be between 8–12 characters"
  - **Start UI actions with a verb**: This makes buttons and links more actionable. Use "Create user" instead of "New
    user".
  - **Give examples in placeholder text**: Use "Example: 2025-01-01" or "name@example.com" rather than an instruction
    like "Enter your email".
  - **Never write "something(s)"**: Always pluralize dynamically: "1 user" instead of "1 user(s)".
- Specific terms
  - **Folder vs directory**: We know these mean the same. We allow both. Use whichever feels better in each situation.
    Like, on the backend, listing "folders" with `readdir` feels wrong, but also, "folder" comes more natural on the
    front-end and end-user docs.

## Code

### Comments

Only add JSDoc that actually adds info. No tautologies.

- ✅ Add meaningful comments for public functions, methods, and types to help the next dev.
- ❌ BUT DO NOT use JSDoc for stuff like `Gets the name` for a function called `getName` :D
- ⚠️ Before adding JSDoc, try using a more descriptive name for the function/param/variable.
- ❌ DO NOT repeat TypeScript types in `@param`/`@returns`.
- ✅ USE JSDoc to mark caveats, tricky/unusual solutions, formats (`YYYY-MM-DD`), and constraints (`must end with /`)

### TypeScript

- Only functional components and modules. No classes.
- Don't use `any` type. ESLint will error.
- Prefer functional programming (map, reduce, some, forEach) and pure functions wherever it makes sense.
- Use `const` for everything, unless it makes the code unnecessarily verbose.
- Start function names with a verb, unless unidiomatic in the specific case.
- Use `camelCase` for variable and constant names, including module-level constants.
- Put constants closest to where they are used. If only used in one function, put it in that function.
- For maps, try to name them like `somethingToSomethingElseMap`. That avoids unnecessary comments.
- Keep interfaces minimal: only export what you must export.

### Svelte 5

- `$state()` can only live in `.svelte` or `.svelte.ts` files, not plain `.ts`.
- Template arrow function closures need explicit type annotations to avoid `any` args from Svelte's event system.
- When extracting logic from `.svelte` to `.ts`, use callback-based deps (getters) rather than threading reactive state.

### Rust

- Max 120 char lines, 4-space indent, cognitive complexity threshold: 15, enforced by clippy.

### CSS

- `html { font-size: 16px; }` is set so `1rem = 16px`. Use `px` by default but can use `rem` if it's more descriptive.
- Use variables for colors, spacing, and the such, in `app.css`.
- Always think about accessibility when designing, and dark + light modes.
- For the full design system (color tokens, typography scale, spacing, component patterns), see
  [design-system.md](design-system.md).

### Icons

We use [UnoCSS](https://unocss.dev/) with the [Icons preset](https://unocss.dev/presets/icons) for UI icons. This
renders icons as **pure CSS** (no JS components, no runtime overhead). The icon data comes from the
[Iconify](https://iconify.design/) ecosystem. We currently use the **Lucide** icon set (`@iconify-json/lucide`).

#### How it works

UnoCSS generates CSS at build time. For monochrome icons, the SVG becomes a CSS `mask-image` with
`background-color: currentColor`, so the icon inherits the text color of its parent. Config:
`apps/desktop/uno.config.ts`.

#### Finding icons

1. Go to [icones.js.org](https://icones.js.org/) and select the **Lucide** collection to stay visually consistent
2. Search by keyword (for example, "warning", "folder", "check")
3. When you find a candidate, its class name is `i-lucide:{icon-name}` (for example, `i-lucide:triangle-alert`)
4. **Always pick icons from the same set** (Lucide) for visual cohesion (consistent stroke width and style)

If you're an AI agent looking for icons: search at `https://icones.js.org/collection/lucide?s={search+terms}`, suggest
candidates to the user with the search URL and terms so they can browse and pick, then use the chosen icon name.

#### Using icons in templates

Icons are plain `<span>` elements with a UnoCSS class. **No imports needed.**

```svelte
<!-- Basic usage — inherits parent text color -->
<span class="i-lucide:triangle-alert"></span>

<!-- With explicit size (use px via scoped CSS, not inline em) -->
<span class="i-lucide:hourglass my-icon-class"></span>
```

#### Sizing

UnoCSS icons default to `1em × 1em` (scales with font size). We override this with **explicit px sizes via scoped CSS
classes**, not inline styles, to keep sizing predictable and consistent:

```svelte
<span class="i-lucide:circle-alert detail-icon"></span>

<style>
    .detail-icon {
        width: 12px;
        height: 12px;
    }
</style>
```

#### Coloring

Icons use `currentColor` by default — they inherit the parent's text color. To color an icon:

- **Preferred**: Set `color` on the parent element, or use a scoped CSS class on the icon
- **For accent color**: Use a scoped class with a stylelint disable comment (because `color: var(--color-accent)` is
  disallowed by default for a11y reasons — it has insufficient contrast as text):
  ```css
  .my-icon {
    /* stylelint-disable-next-line declaration-property-value-disallowed-list -- icon indicator, not body text */
    color: var(--color-accent);
  }
  ```
- **For semantic colors**: Use `var(--color-warning)`, `var(--color-error)`, etc. directly — these aren't restricted

#### Adding a new icon set

If Lucide doesn't have what you need, install another Iconify set (for example, `pnpm add -D @iconify-json/mdi` for
Material Design Icons). Then add it to the `ignoreDependencies` list in `knip.json` and to the `allowedUndefinedClasses`
in `scripts/check-css-unused/allowlist.go`. Prefer sticking to one set per context for visual consistency.

#### Checklist for adding a new icon

1. Find the icon at [icones.js.org](https://icones.js.org/) in the Lucide collection
2. Use it as `<span class="i-lucide:{name} my-class"></span>`
3. Set size via a scoped CSS class (not inline style, not `em`)
4. Set color via scoped CSS or parent inheritance
5. Add the class name to `scripts/check-css-unused/allowlist.go` → `allowedUndefinedClasses`

## Design

See [design-principles.md](design-principles.md) for product design values (UX, accessibility, cancellation, platform
behavior). Read it when designing features or making UX decisions.
