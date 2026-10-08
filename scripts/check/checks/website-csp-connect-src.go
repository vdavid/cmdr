package checks

import (
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// websiteSecurityHeadersPath is the one nginx snippet every location block includes, so the CSP
// this check reads is the CSP production serves.
const websiteSecurityHeadersPath = "apps/website/nginx-security-headers.conf"

// RunWebsiteCSPConnectSrc fails when website code fetches an origin the production CSP's
// `connect-src` doesn't allow. The browser blocks such a request before it leaves the page, and
// the calling code sees only a generic network failure, so nothing in dev (which serves no CSP)
// or in the API's logs ever shows it. The blog's like button shipped that way and never worked
// in production; the `?r=` link-code lookup was blocked the same way.
func RunWebsiteCSPConnectSrc(ctx *CheckContext) (CheckResult, error) {
	headersBytes, err := os.ReadFile(filepath.Join(ctx.RootDir, websiteSecurityHeadersPath))
	if err != nil {
		return CheckResult{}, fmt.Errorf("could not read %s: %w", websiteSecurityHeadersPath, err)
	}
	connectSrc, err := cspConnectSources(string(headersBytes))
	if err != nil {
		return CheckResult{}, fmt.Errorf("%s: %w", websiteSecurityHeadersPath, err)
	}

	websiteDir := filepath.Join(ctx.RootDir, "apps", "website")
	var violations []string
	targets := 0
	for _, sub := range []string{"src", "public"} {
		root := filepath.Join(websiteDir, sub)
		walkErr := filepath.WalkDir(root, func(path string, d fs.DirEntry, err error) error {
			if err != nil {
				return err
			}
			if d.IsDir() {
				// The dev-only blog editor never ships, and it talks to the Vite dev server. `src/build/`
				// holds build-time-only modules (imported from component frontmatter, never from a
				// client `<script>`), whose fetches run in Node during `astro build`, not under the CSP.
				if (filepath.Base(path) == "dev" || filepath.Base(path) == "build") && filepath.Dir(path) == filepath.Join(websiteDir, "src") {
					return filepath.SkipDir
				}
				return nil
			}
			if !isWebsiteScriptSource(path) {
				return nil
			}
			fileTargets, fileViolations, readErr := checkFileFetches(ctx.RootDir, path, connectSrc)
			targets += fileTargets
			violations = append(violations, fileViolations...)
			return readErr
		})
		if walkErr != nil {
			return CheckResult{}, fmt.Errorf("walking %s: %w", root, walkErr)
		}
	}

	if len(violations) > 0 {
		sort.Strings(violations)
		msg := "website code fetches origins the production CSP blocks:"
		for _, v := range violations {
			msg += "\n  - " + v
		}
		msg += "\nAdd the origin to `connect-src` in " + websiteSecurityHeadersPath +
			" (the browser drops the request silently otherwise), or fetch a same-origin path."
		return CheckResult{}, fmt.Errorf("%s", msg)
	}
	return Success(fmt.Sprintf("%d fetch %s allowed by connect-src", targets, Pluralize(targets, "target", "targets"))), nil
}

// checkFileFetches returns how many fetch targets one file has and the ones `connect-src` rejects.
func checkFileFetches(rootDir, path string, connectSrc []string) (int, []string, error) {
	content, err := os.ReadFile(path)
	if err != nil {
		return 0, nil, err
	}
	rel, _ := filepath.Rel(rootDir, path)
	targets := findFetchTargets(string(content))
	var violations []string
	for _, t := range targets {
		if t.unresolved != "" {
			violations = append(violations, fmt.Sprintf("%s:%d: can't tell which origin `%s` fetches; "+
				"use a literal URL or a same-file `const %s = 'https://…'`", rel, t.line, t.unresolved, t.unresolved))
			continue
		}
		if !cspAllows(connectSrc, t.origin) {
			violations = append(violations, fmt.Sprintf("%s:%d: fetches %s, which `connect-src` doesn't allow",
				rel, t.line, t.origin))
		}
	}
	return len(targets), violations, nil
}

func isWebsiteScriptSource(path string) bool {
	if strings.Contains(path, ".test.") || strings.Contains(path, ".spec.") {
		return false
	}
	switch filepath.Ext(path) {
	case ".astro", ".ts", ".js", ".mjs", ".svelte":
		return true
	}
	return false
}

var cspHeaderRE = regexp.MustCompile(`add_header\s+Content-Security-Policy\s+"([^"]+)"`)

// cspConnectSources returns the sources `connect-src` allows, falling back to `default-src` the
// way browsers do when `connect-src` is absent.
func cspConnectSources(conf string) ([]string, error) {
	matches := cspHeaderRE.FindAllStringSubmatch(conf, -1)
	if len(matches) != 1 {
		return nil, fmt.Errorf("expected exactly one Content-Security-Policy header, found %d", len(matches))
	}
	directives := map[string][]string{}
	for _, part := range strings.Split(matches[0][1], ";") {
		fields := strings.Fields(part)
		if len(fields) == 0 {
			continue
		}
		directives[strings.ToLower(fields[0])] = fields[1:]
	}
	if sources, ok := directives["connect-src"]; ok {
		return sources, nil
	}
	if sources, ok := directives["default-src"]; ok {
		return sources, nil
	}
	return nil, fmt.Errorf("the CSP has neither connect-src nor default-src")
}

// cspAllows reports whether a source list admits `origin` (`scheme://host`, or "" for a
// same-origin relative URL). It covers the source forms our CSP uses: 'self', exact origins, and
// `https://*.example.com` wildcards.
func cspAllows(sources []string, origin string) bool {
	for _, s := range sources {
		if s == "'self'" && origin == "" {
			return true
		}
		if origin == "" {
			continue
		}
		if strings.EqualFold(s, origin) {
			return true
		}
		if scheme, rest, ok := strings.Cut(s, "://*."); ok {
			if strings.HasPrefix(origin, scheme+"://") && strings.HasSuffix(origin, "."+rest) {
				return true
			}
		}
	}
	return false
}

type fetchTarget struct {
	line int
	// origin is `scheme://host`, or "" for a relative (same-origin) URL.
	origin string
	// unresolved names the identifier the URL starts with when it isn't a same-file string const.
	unresolved string
}

var (
	// The start of a network call's first argument: a quoted/template URL or an identifier.
	fetchCallRE = regexp.MustCompile(`\b(?:fetch|sendBeacon|new\s+EventSource|new\s+WebSocket)\(\s*` +
		"(?:(['\"`])([^'\"`]*)|([A-Za-z_$][\\w$]*))")
	literalOriginRE = regexp.MustCompile(`^([a-z][a-z0-9+.-]*://[^/?#'"` + "`" + `$]+)`)
	templateIdentRE = regexp.MustCompile(`^\$\{\s*([A-Za-z_$][\w$]*)\s*\}`)
)

func findFetchTargets(content string) []fetchTarget {
	var targets []fetchTarget
	for _, m := range fetchCallRE.FindAllStringSubmatchIndex(content, -1) {
		line := strings.Count(content[:m[0]], "\n") + 1
		var ident string
		if m[4] >= 0 {
			url := content[m[4]:m[5]]
			quote := content[m[2]:m[3]]
			if quote == "`" {
				if im := templateIdentRE.FindStringSubmatch(url); im != nil {
					ident = im[1]
				}
			}
			if ident == "" {
				targets = append(targets, fetchTarget{line: line, origin: literalOrigin(url)})
				continue
			}
		} else {
			ident = content[m[6]:m[7]]
		}
		if origin, ok := resolveConstOrigin(content, ident); ok {
			targets = append(targets, fetchTarget{line: line, origin: origin})
		} else {
			targets = append(targets, fetchTarget{line: line, unresolved: ident})
		}
	}
	return targets
}

func literalOrigin(url string) string {
	if m := literalOriginRE.FindStringSubmatch(url); m != nil {
		return strings.ToLower(m[1])
	}
	return ""
}

// resolveConstOrigin finds `const|let|var <ident> = '<url>'` in the same file and returns the
// URL's origin.
func resolveConstOrigin(content, ident string) (string, bool) {
	re := regexp.MustCompile(`\b(?:const|let|var)\s+` + regexp.QuoteMeta(ident) + "\\s*=\\s*['\"`]([^'\"`]*)['\"`]")
	m := re.FindStringSubmatch(content)
	if m == nil {
		return "", false
	}
	return literalOrigin(m[1]), true
}
