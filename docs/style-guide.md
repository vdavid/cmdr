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
    - **Don't use permissive language**: Give users confidence. "Add repos and start searching" not "Add repos and you
      can start searching."
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
- UI
    - **Error messages**: Keep conversational, positive, actionable, and specific. Never use the words "error" or
      "failed" — we wouldn't say those in conversation. Suggest a next step.
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

## Design

- Always make features extremely user-friendly.
- Always apply radical transparency: make the internals of what's happening available. Hide the details from the surface
  so the main UI is not cluttered.
- For longer processes: 1. show a progress indicator (an anim), 2. a progress bar and counter if we know the end state
  (for example, how many files we're loading), and 3. a time estimate if we have a guess how long it'll take.
- Always keep accessibility in mind. Features should be available to people with impaired vision, hearing, and cognitive
  disabilities.
- All actions longer than ~1 second should be immediately cancelable, canceling not just the UI but any background
  processes as well, to avoid wasting the user's resources.
- Write _elegant_ code. Not quick code, not overengineered code, but elegant code. If you need to choose between a small
  refactor that leads to a slightly better architecture or a larger refactor that leads to a near-perfect architecture,
  choose the larger refactor.
- When shortcuts are available for a feature, always display the shortcut in a tooltip or somewhere less prominent than
  the main UI.
- **Platform-native, not generic.** The app should look and feel as if it was specifically made for the user's OS. Never
  generalize user-facing text, labels, or behavior to be "cross-platform" — instead, fork by OS. On macOS, say "Finder",
  "Trash", "System Settings". On Linux, say "file manager", "Trash" (FreeDesktop spec), and use DE-specific terminology
  where possible. Windows (later) gets its own native terms too. This applies to error messages, menu labels, tooltips,
  and any user-visible string. Use `isMacOS()` / `cfg(target_os)` to branch — a few extra lines of platform-specific
  text are always better than one watered-down generic string.

## Git

### Commit messages

- Title: Optional prefix like "Bugfix: ", "Docs: ", "Tooling: ", "File viewer: ", etc. Max 50 chars. Capture the IMPACT
  of the change, not the tech details.
- Body: A few bullets of details if needed. No word wrap — don't hard-wrap body lines at 72 chars or any other width.
  Let the terminal/viewer wrap naturally. Enclose entities in ``. No co-author.

### PRs

- Use the PR title to summarize the changes in a casual/informal tone. Be information dense and concise.
- In the desc., write a thorough, organized, but concise, often bulleted list of the changes. Use no headings.
- At the bottom of the PR description, use a single "## Test plan" heading, in which, explain how the changes were
  tested. Assume that the changes were also tested manually if it makes sense for the type of changes.

## Docs

### Guides

See [this diff](https://github.com/vdavid/cmdr/commit/13ad8f3#diff-795210f) before writing guides. This diff shows how
we like our guides formatted. (Before: was AI-written. After: matching our standards for conciseness and clarity.)
