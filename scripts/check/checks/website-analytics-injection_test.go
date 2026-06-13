package checks

import (
	"strings"
	"testing"
)

// goodAnalyticsHTML mirrors the shape Astro emits for a healthy build: the
// is:inline injector bodies are raw JS (a define:vars prelude, then the
// __cmdrRReady-gated createElement injection).
const goodAnalyticsHTML = `<!doctype html><html><head>
<script>window.__cmdrRReady = new Promise(function (resolve) { resolve() })</script>
<script>(function(){const umamiHost = "/u";
const umamiId = "00000000-0000-0000-0000-000000000000";
;(window.__cmdrRReady || Promise.resolve()).then(function () {
  var s = document.createElement('script')
  s.src = umamiHost + '/mami'
  s.setAttribute('data-website-id', umamiId)
  document.head.appendChild(s)
})})();</script>
</head><body>
<script>(function(){const phKey = "phc_test";
;(window.__cmdrRReady || Promise.resolve()).then(function () {
  var s = document.createElement('script')
  s.src = '/scripts/posthog-init.js'
  s.setAttribute('data-ph-key', phKey)
  document.body.appendChild(s)
})})();</script>
</body></html>`

// brokenAnalyticsHTML mirrors what ships when an is:inline body is wrapped in a
// literal Astro {`...`} expression: the JS becomes a dead block + bare template
// literal, so the .then() never runs. createElement/data-website-id are still
// present TEXTUALLY (inside the dead string), which is exactly why the negative
// `{`-then-backtick signal — not their mere presence — is what catches the bug.
const brokenAnalyticsHTML = `<!doctype html><html><head>
<script>window.__cmdrRReady = new Promise(function (resolve) { resolve() })</script>
<script>(function(){const umamiHost = "/u";
const umamiId = "00000000-0000-0000-0000-000000000000";

  {` + "`" + `
  ;(window.__cmdrRReady || Promise.resolve()).then(function () {
    var s = document.createElement('script')
    s.src = umamiHost + '/mami'
    s.setAttribute('data-website-id', umamiId)
    document.head.appendChild(s)
  })
  ` + "`" + `}
})();</script>
</head><body></body></html>`

func TestAssertAnalyticsInjectionAcceptsHealthyHTML(t *testing.T) {
	if v := assertAnalyticsInjection(goodAnalyticsHTML); len(v) != 0 {
		t.Fatalf("expected no violations for healthy HTML, got: %v", v)
	}
}

func TestAssertAnalyticsInjectionCatchesInertWrapper(t *testing.T) {
	v := assertAnalyticsInjection(brokenAnalyticsHTML)
	if len(v) == 0 {
		t.Fatal("expected a violation for the inert `{`...`}` wrapper, got none")
	}
	joined := strings.Join(v, "\n")
	if !strings.Contains(joined, "template literal") {
		t.Fatalf("expected the inert-wrapper violation, got: %v", v)
	}
}

func TestAssertAnalyticsInjectionCatchesMissingInjectors(t *testing.T) {
	// A build with the PUBLIC_* env unset: the gated branch never renders, so
	// none of the injection markers are present. Every positive assertion fires.
	emptyHTML := `<!doctype html><html><head></head><body></body></html>`
	v := assertAnalyticsInjection(emptyHTML)
	if len(v) < 4 {
		t.Fatalf("expected all positive assertions to fire on empty HTML, got %d: %v", len(v), v)
	}
}

func TestScriptBodyHasInertWrapperIgnoresNonScriptContent(t *testing.T) {
	// The `{`-then-backtick sequence outside a <script> body (e.g. a blog post
	// quoting code) must not trip the detector.
	html := "<!doctype html><html><body><p>Astro renders {`hello`} literally.</p></body></html>"
	if scriptBodyHasInertWrapper(html) {
		t.Fatal("inert-wrapper detector tripped on non-script page content")
	}
}
