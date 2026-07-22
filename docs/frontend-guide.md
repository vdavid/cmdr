### UI style guide

- **Error messages**: Keep conversational, positive, actionable, and specific. Avoid the words "error" or "failed".
  Suggest a next step.
  - "Couldn't rename the file. Try again?" not "Error: Rename operation failed."
  - "Password must contain at least 12 characters" not "Password format is invalid (minimum 12 characters)"
  - "Sorry, we couldn't save your changes. Try again?" not "Failed to save changes."
- **Success messages**: Talk about the user, not the action. Make success implicit and warm.
  - "Your files moved to ~/Documents" not "Move operation completed successfully."
  - "Shortcut saved. It's ready to use!" not "Shortcut successfully created."
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
- **Give examples in placeholder text**: Use "Example: 2025-01-01" or "name@example.com" rather than an instruction like
  "Enter your email".
- **Never write "something(s)"**: Always pluralize dynamically: "1 user" instead of "1 user(s)".

## Coding

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

### CSS

- `html { font-size: 16px; }` is set so `1rem = 16px`. Use `px` by default but can use `rem` if it's more descriptive.
- Use variables for colors, spacing, and the such, in `app.css`.
- Always think about accessibility when designing, and dark + light modes.
- For the full design system (color tokens, typography scale, spacing, component patterns), see `design-system.md`.

### Icons

We use `unplugin-icons` + `@iconify-json/lucide` (Lucide set, rendered as inline SVG Svelte components). Before adding
or styling an icon, read `guides/icons.md`: finding icons, template usage, sizing, coloring, and the add-an-icon
checklist.
