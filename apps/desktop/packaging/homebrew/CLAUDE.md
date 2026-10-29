# Homebrew cask

`cmdr.rb` is the source of truth for the cask's **shape** (`url`, `livecheck`, `depends_on`, `app`, `binary`,
`uninstall`, `zap`). ❗ The `uninstall script:` block clears the machine-wide `NSFileViewer` key when it still holds a
Cmdr bundle id, and that lives in `uninstall` rather than `zap` on purpose: `docs/guides/homebrew-cask.md`. The live
channel is the personal tap [`vdavid/homebrew-tap`](https://github.com/vdavid/homebrew-tap), whose `Casks/cmdr.rb` must
carry this exact shape. Release CI (`release-pipeline.yml`'s `bump-tap` job) copies this file over the tap's at each
release and fills in `version` + `sha256`, so make shape edits here only: they ship with the next release, and a hand
edit in the tap is overwritten. Don't hand-bump `version` / `sha256` (CI does it). The tap stays canonical until
`vdavid/cmdr` clears Homebrew's notability bar, at which point a resubmission to `Homebrew/homebrew-cask` becomes the
canonical home and the tap retires.

Before touching `cmdr.rb`, read `docs/guides/homebrew-cask.md`: it explains the non-obvious constraints (why the `url`
must not be a `github.com` one, how the tap bump works, how to test in a throwaway tap) and the resubmission process.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
