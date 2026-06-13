package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
)

// Analytics-injection env values. Placeholders only: they make Astro emit the
// `import.meta.env.PUBLIC_*`-gated Umami/PostHog injection branches in
// Layout.astro (the website-build check builds WITHOUT these, so those branches
// never appear in its dist/ — the exact reason a broken injection shipped
// unnoticed). The PostHog key is a syntactically valid `phc_` placeholder and
// the Umami id a fake; neither hits a real backend (we only assert on built
// HTML, no browser, no network).
var analyticsBuildEnv = []string{
	"PUBLIC_UMAMI_HOST=/u",
	"PUBLIC_UMAMI_WEBSITE_ID=00000000-0000-0000-0000-000000000000",
	"PUBLIC_POSTHOG_KEY=phc_testplaceholderkey000000000000000000",
	"PUBLIC_POSTHOG_HOST=/ph",
	// astro build sets import.meta.env.PROD=true, which makes pricing.astro throw
	// on sandbox Paddle creds unless this escape hatch is set (the documented
	// local/CI build flag, see apps/website/.env.example). We don't exercise
	// checkout here; this just lets the analytics build complete.
	"PUBLIC_PADDLE_ALLOW_SANDBOX=true",
}

// analyticsBuildOutDir is a dedicated output dir so this build never clobbers
// the env-free dist/ that website-build, html-validate, bundle-size, and e2e
// all consume. Astro empties its outDir on each build, so reuse is safe.
const analyticsBuildOutDir = "dist-analytics"

// RunWebsiteAnalyticsInjection builds the website WITH the analytics PUBLIC_*
// env set and asserts the Umami + PostHog injectors survive, well-formed, into
// the built HTML. It guards a whole bug class: the injectors are
// `<script is:inline>` blocks whose body is raw JS, and a refactor once wrapped
// that body in a literal Astro `{`...`}` expression. Astro does NOT evaluate
// `{...}` inside an is:inline body, so the JS shipped as inert literal text and
// BOTH analytics layers were silently dead in prod (no error, no script tag).
//
// The website-build / html-validate lane can't catch this: it builds without
// the PUBLIC_* env, so the gated injection branch never renders at all. This
// check supplies the env, so the branch renders and we can assert on it.
func RunWebsiteAnalyticsInjection(ctx *CheckContext) (CheckResult, error) {
	websiteDir := filepath.Join(ctx.RootDir, "apps", "website")

	cmd := exec.Command("pnpm", "exec", "astro", "build", "--outDir", analyticsBuildOutDir)
	cmd.Dir = websiteDir
	cmd.Env = append(os.Environ(), analyticsBuildEnv...)
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("analytics build failed\n%s", indentOutput(output))
	}

	indexPath := filepath.Join(websiteDir, analyticsBuildOutDir, "index.html")
	htmlBytes, err := os.ReadFile(indexPath)
	if err != nil {
		return CheckResult{}, fmt.Errorf("could not read built %s: %w", indexPath, err)
	}
	html := string(htmlBytes)

	violations := assertAnalyticsInjection(html)
	if len(violations) > 0 {
		msg := "analytics injection broken in built HTML (the env-gated branch rendered wrong):"
		for _, v := range violations {
			msg += "\n  - " + v
		}
		msg += "\nLikely cause: an is:inline injector body got wrapped in a literal Astro `{`...`}` expression, " +
			"which Astro ships as inert text instead of executable JS. Author the body as plain JS " +
			"(see apps/website/src/layouts/Layout.astro and its analytics gotcha in CLAUDE.md)."
		return CheckResult{}, fmt.Errorf("%s", msg)
	}

	return Success("Umami + PostHog injectors present and well-formed in built HTML"), nil
}

// assertAnalyticsInjection returns a list of human-readable violations for the
// built index.html. Empty means the analytics injection is healthy. The checks
// are deliberately split into positive (the injection must be present and
// executable) and negative (the inert literal-wrapper pattern must be absent)
// so a failure points at exactly what regressed.
func assertAnalyticsInjection(html string) []string {
	var violations []string

	// Positive: Umami is injected via a real createElement('script') that sets
	// the src and the data-website-id — not printed as literal text. In the
	// working build the is:inline body is raw JS, so these appear verbatim.
	if !strings.Contains(html, "createElement('script')") &&
		!strings.Contains(html, `createElement("script")`) {
		violations = append(violations,
			"no `createElement('script')` found — the Umami/PostHog injectors didn't render as executable JS")
	}
	if !strings.Contains(html, "data-website-id") {
		violations = append(violations,
			"no `data-website-id` found — the Umami injector is missing (env not picked up, or branch broken)")
	}

	// Positive: PostHog is injected via the extracted init script.
	if !strings.Contains(html, "/scripts/posthog-init.js") {
		violations = append(violations,
			"no `/scripts/posthog-init.js` reference — the PostHog injector is missing")
	}

	// Positive: both injectors gate on the `?r=` expansion promise, so the
	// pageview records the expanded URL, not the raw `?r=`. (The early head
	// script defines it; the consumers reference it.)
	if !strings.Contains(html, "__cmdrRReady") {
		violations = append(violations,
			"no `__cmdrRReady` reference — the `?r=` ordering gate is missing, analytics would record the raw `?r=`")
	}

	// Negative: catch the exact bug class. An is:inline injector body is raw JS.
	// When it's mistakenly wrapped in a literal Astro `{`...`}` expression, Astro
	// evaluates the JSX child and ships a `{`...`}` block whose only content is a
	// bare template-literal string — so the JS inside becomes a dead string the
	// browser never runs. The HTML signature is a `{` immediately followed
	// (ignoring whitespace) by a backtick inside a <script> body. A well-authored
	// plain-JS inline body never produces that sequence; the working build has
	// zero, the broken build has exactly one.
	if scriptBodyHasInertWrapper(html) {
		violations = append(violations,
			"a <script> body contains a `{`...`}` wrapper around a template literal — "+
				"the injector JS is a dead string, not executable code (the exact bug this check guards)")
	}

	return violations
}

// inertWrapperRE matches an open-brace followed (across whitespace, including
// newlines) by a backtick: the literal text Astro emits when an is:inline
// script body is wrapped in `{`...`}`. `(?s)` lets `.`/`\s` span newlines.
var inertWrapperRE = regexp.MustCompile("(?s)\\{\\s*`")

// scriptBodyHasInertWrapper reports whether any <script>...</script> body
// contains the `{`-then-backtick inert-wrapper signature. Scoped to script
// bodies so unrelated page content (a blog post quoting `{` then a backtick)
// can't trip it.
func scriptBodyHasInertWrapper(html string) bool {
	rest := html
	for {
		open := strings.Index(rest, "<script")
		if open < 0 {
			return false
		}
		tagEnd := strings.IndexByte(rest[open:], '>')
		if tagEnd < 0 {
			return false
		}
		bodyStart := open + tagEnd + 1
		close := strings.Index(rest[bodyStart:], "</script")
		if close < 0 {
			return false
		}
		body := rest[bodyStart : bodyStart+close]
		if inertWrapperRE.MatchString(body) {
			return true
		}
		rest = rest[bodyStart+close:]
	}
}
