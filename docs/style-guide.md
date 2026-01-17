# Style guide

Writing and code styles.

## Writing

- Wording
    - **Use a friendly style**: Make all texts informal, friendly, encouraging, and concise.
    - **Use active voice**: Prefer an active voice rather than passive when writing text.
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
    - **Use en dashes and em dashes**: en dash for ranges, em dash for combining thoughts.
    - **Use colon for lists**: Use the format I used in this list you're reading right now.
    - **Spell out numbers one through nine.** Use numerals for 10+.
    - **Use ISO dates**: Use YYYY-MM-DD wherever it makes sense.
- UI
    - Make **error messages** positive, actionable, and specific.
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

Only add JSDoc that actually adds info.

- ✅ Add meaningful comments for public functions, methods, and types to help the next dev.
- ❌ BUT DO NOT use JSDoc for stuff like `Gets the name` for a function called `getName` :D
- ❌ DO NOT repeat TypeScript types in `@param`/`@returns`.
- ✅ USE JSDoc to mark caveats, tricky/unusual solutions, formats (`YYYY-MM-DD`), and constraints (`must end with /`)
- ⚠️ Before adding JSDoc, try using a more descriptive name for the function/param/variable.

### TypeScript

- Only use functional components and modules. No classes anywhere.
- Prefer functional programming (map, reduce, some, forEach) and pure functions wherever it makes sense.
- Use `const` for everything, unless it makes the code unnecessarily verbose.
- Start function names with a verb, unless unidiomatic in the specific case.
- Use `camelCase` for variable and constant names, including module-level constants.
- Put constants closest to where they are used. If a constant is only used in one function, put it in that function.
- For maps, try to name them like `somethingToSomeethingElseMap`. That avoids unnecessary comments.
- Keep interfaces minimal: only export what you must export.

### Rust

- Max line width: 120 characters
- 4 spaces indentation
- Cognitive complexity threshold: 15 (enforced by clippy)

### CSS

- `html { font-size: 16px; }` is set so `1rem = 16px`. Use `px` by default but can use `rem` if it's more descriptive.
- Use variables for colors, spacing, and the such, in `app.css`.
- Always think about accessibility when designing, and dark+light modes.

## Git

### Commit messages

The first line is max 50 characters. Examples: "Add new feature X", "Frontend: Fix Save button size on the Settings
page"

Then a blank line. Then a more detailed description if needed, as a form of a concise bulleted list, or free text with
meaningful extra details on what the commit does.

### PRs

- Use the PR title to summarize the changes in a casual/informal tone. Be information dense and concise.
- In the desc., write a thorough, organized, but concise, often bulleted list of the changes. Use no headings.
- At the bottom of the PR description, use a single "## Test plan" heading, in which, explain how the changes were
  tested. Assume that the changes were also tested manually if it makes sense for the type of changes.

## Docs

### Guides

See [this diff](https://github.com/vdavid/cmdr/commit/13ad8f3#diff-795210f) before writing guides. This diff shows how
we like our guides formatted. (Before: was AI-written. After: matching our standards for conciseness and clarity.)   