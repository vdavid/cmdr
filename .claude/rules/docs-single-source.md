# Single-source docs; don't duplicate a mechanism

A load-bearing technical claim or mechanism lives in exactly ONE canonical doc: the module doc or colocated `DETAILS.md`
nearest the code that implements it. Everywhere else points to it by path, never restates it. Copied prose rots
independently: we once had the same Tahoe FDA mechanism described (wrongly) in three places at once and had to fix all
three.

- **`architecture.md` is a map: what + where + pointer, never how.** It says a subsystem exists, where it lives, and
  links its `DETAILS.md`/module doc. It must not describe mechanism, flows, or triggers. The map is the doc most prone
  to accreting "how": keep it terse.
- **One canonical home per mechanism.** Same idea as `use-codegraph` ("don't transcribe what codegraph owns"), extended
  from symbol locations to behavioral facts. Designate the home; everywhere else links.
- **Evidence-anchor volatile claims.** Any claim about OS/external behavior, a version, or an empirical finding carries
  `(verified on <version/env>, <method>, <date-or-commit>)`. OS behavior drifts under undated confident claims and turns
  them into landmines.
