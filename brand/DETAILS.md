# Details

Depth and rationale for this area. `CLAUDE.md` holds only the must-knows that prevent silent breakage; everything else
(architecture narrative, data flows, decision rationale, edge-case catalogs) lives here.

## App-directory listings (`listings/`)

Every app directory Cmdr gets submitted to (MacUpdate, AlternativeTo, and whatever comes next) gets one markdown file
named after the site. The file is the submission itself, not notes about it: every field of that site's form, with the
value ready to paste in a fenced block, in the form's own order, plus the paths of the icon and screenshots that were
uploaded.

Why the repo and not a personal knowledge base: these listings quote the same claims the README, website, and `copy/`
make, and they go stale on the same events (a release, a pricing change, a screenshot reshoot). Living next to the
assets they reference is what lets one refresh pass catch all of them. Everything in these files is public marketing
copy by definition, so a public repo costs nothing. Keep portal credentials, reviewer email threads, and unlaunched
pricing out.

Conventions:

- Note at the top whether the listing is submitted or still a draft, which version it describes, and any format the site
  imposes (MacUpdate's description and changelog fields take HTML and ban pricing text, for example).
- Download URLs go through `getcmdr.com/download/latest/<arch>?ref=<site>`: always the current release (so a listing
  never goes stale on a release), and the `ref` attributes the downloads to that directory in the analytics dashboard.
  Mechanism: `apps/api-server/src/telemetry/DETAILS.md` § Download tracking.
- Record open decisions (price field, developer name) as the value actually submitted, not as a question. The discussion
  belongs in the commit message. The exception is a "Pending updates" section on a live listing: when the site is known
  to be showing something stale, park the concrete replacement value there so the next edit pass is paste-only.
