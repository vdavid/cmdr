# Lane markers

Lets a CI lane skip when the exact files it reads already passed in an earlier trusted run, so a docs fix pushed onto a
green Rust branch doesn't rerun the macOS, E2E, and Rust lanes. A pure function of the commit's tree plus one Actions
cache lookup.

## Module map

- `match.go`: dorny/paths-filter's matching (picomatch subset, all three quantifiers). `workflow.go`: finds the filters
  and the lanes in the workflows. `hash.go`: a lane's input set and hash. `markers.go`: the trust rules and `decide`.
  `verify.go`: `CI OK`'s lane rule. `main.go`: the `hash`, `plan`, and `verify` subcommands.

## Must-knows

- **A lane is a job that reads its own `fromJSON(needs.changes.outputs.lanes)['<id>'].key`** (the marker save). Its
  inputs: the files its filters match at the commit, its workflow file, every local `uses:` it reaches (jobs it `needs`
  included), the toolchain pins, and `scripts/check/**`. Over-inclusion costs a rerun; under-inclusion is the one way to
  report green for untested code.
- **Only `pull_request` runs trust markers**, from `main`'s scope and (same-repo heads only) the PR's own. Pushes,
  dispatches, and `run_all` ignore them.
- **Only a lane's last step saves its marker**, on success. ❌ Never save a `lane-green-*` key anywhere else.
- **`CI OK` runs `verify`**: a lane skipped while its `if:` filter is on is red unless the plan verified it AND this job
  recomputes the same key AND finds the marker in a trusted scope.
- **Unsupported glob syntax fails loudly** (braces, brackets, extglobs). Extend `match.go` against picomatch first.
- ❌ Don't let `CI OK` read a lane's plan entry via `fromJSON(...)['x']`: it reads `toJSON(needs)`, and the parser would
  take it for a job saving another lane's marker.
- `go run . hash -files` (here) prints each lane's key and inputs at `HEAD`.

Trust argument, store choice, the one risk and its guards, and the per-lane wiring: `DETAILS.md`.
