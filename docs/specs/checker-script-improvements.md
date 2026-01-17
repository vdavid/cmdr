I want to work with you on the checker script, which is in `scripts/check/`. I want these improvements:

1. I want it to be a bit more modular. It seems better if each check has its own file. I want them to live in one dir, in `scripts/check/checks/`, and the files named after the
   category they fall in, and the check itself, like `{app}-{tech-optional}-{check}.go`, e.g. `desktop-rust-clippy.go`, `desktop-svelte-knip.go`, `website-prettier.go`,
   `license-server-prettier.go`, etc.

2. I want checks to implement a common interface. They'd get the *CheckContext like they do now, but not just return `error` but also some value when they succeed, probably
   something like (pseudocode): `{resultCode: success|warn, resultText: string}`, and then if the result includes no line break then the main script would output `• {checkname}...
   <green|yellow>{OK|warn}</green|yellow> ({ms}ms) - <green|yellow>{resultText}</green|yellow>`, and if it does have a linebreak then
   ```
   • {checkname}... <green|yellow>{OK|warn}</green|yellow> ({ms}ms)
     <green|yellow>{resultText}</green|yellow>
   ```

3. Instead of the current display of categories like
   ```
   🦀 Rust checks (desktop)...
     • rustfmt... OK (402ms)
   ```
   I want single line like `Desktop: 🦀 Rust checks / rustfmt... OK (402ms)` so there would be no categories.

4. I want to start running in parallel whatever we can. I want to set up an easy to read dependency graph. I think if we mark for every check to know what other checks it needs to
   wait for, that's intuitive enough. For example, I want ESLint to wait for Prettier so that the formatting is fixed by the time it gets there. An I want the main thread to run
   whatever tests that have no pending dependencies, as many in parallel as reasonable. What Rust's parallel executor thing does it that it auto-sclaes the number of threads to the
   number of CPU cores available. I'd love something like that if it's easy to do in Go, otherwise let's just do 10 or sg.
    - With the parallel runs, make sure that whatever multi-line output the script has (either the success/warning message, or an error message) to be printed immediately after the
      line that has the check name, and do this safely without the race condition between the checks running in parallel.

5. If any of the tests fail, I want the script to continue even if some tests fail, except for dependent scripts. This, unless a `--fail-fast` (or whatever name is idiomatic) arg
   is present.

6. I want the registry.go to be changed to a different format, to hold all checks in one hard-coded array or sg with this format for each check (pseudocode): `{id: string (unique,
   e.g. "license-server-prettier"), displayName: string (not unique, e.g. "prettier", to be displayed together with the app+tech), app: enum<desktop|website|license-server|other>,
   tech:"🦀 Rust"|"🎨 Svelte"|"🚀 Astro"|"⸆⸉ TS"|"", isSlow: boolean, dependendsOn: string[] (a list of IDs can come here)}`

7. I want to introduce an "is slow" property, and only mark the Rust Linux test as slow for now.

8. Slow tests should not be included by default, only if they are added by a `--check` arg or if an `--include-slow` arg is present.

9. I want the list of checks for `--help` to be a dynamically generated rather than a static list.

10. I want the script to keep a list of running checks, and display their IDs, capped at, say 60 chars width with ellipses, like `Waiting for: desktop-rustfmt, desktop-clippy,
    desktop-prettier...`, and this list to always the the last line, and get updated as the script runs.

11. I want each check to output some meaningful short success message, that includes stats on what was done, e.g. `All 12 tests in 4 files passed` or `Fixed formatting in 3 files`.

12. Write docs for the script in `docs/tooling/checker-script.md` that helps the dev (me and agents) use it and maintain it (e.g. adding a new test, etc.).

Please use a task list to make sure you don't forget any of these. No need to go sequentially, go in whatever order you want to go.