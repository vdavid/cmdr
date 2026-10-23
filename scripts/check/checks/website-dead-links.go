package checks

import (
	"fmt"
	"io/fs"
	"net/url"
	"os"
	"path"
	"path/filepath"
	"sort"
	"strings"

	"golang.org/x/net/html"
)

// RunWebsiteDeadLinks fails when a built page links to something the build doesn't contain: an
// empty `href`, an internal page or asset with no file in `dist/`, or a `#fragment` naming no `id`
// on its target page. External URLs are skipped (no network in a check). It reads the BUILT site,
// so it covers blog markdown, `src/lib/` content, and templates alike. A blog post once shipped
// `[background drive index](<>)`, which renders as `href=""`, and nothing noticed.
func RunWebsiteDeadLinks(ctx *CheckContext) (CheckResult, error) {
	distDir := filepath.Join(ctx.RootDir, "apps", "website", "dist")
	if !hasBuildOutput(distDir) {
		return Skipped("dist/ not found (run website-build first)"), nil
	}

	scan, err := scanWebsiteDeadLinks(distDir)
	if err != nil {
		return CheckResult{}, err
	}
	if len(scan.broken) > 0 {
		return CheckResult{}, fmt.Errorf("%d dead %s in the built website\n%s",
			len(scan.broken), Pluralize(len(scan.broken), "link", "links"), indentOutput(strings.Join(scan.broken, "\n")))
	}
	return Success(fmt.Sprintf("%d internal %s across %d %s resolve",
		scan.links, Pluralize(scan.links, "link", "links"), scan.pages, Pluralize(scan.pages, "page", "pages"))), nil
}

type websiteDeadLinkScan struct {
	broken []string // "page: attr=\"value\" (reason)", sorted
	links  int      // internal links checked
	pages  int
}

// websitePageLink is one internal reference found on a page, before resolution.
type websitePageLink struct {
	page  string // dist-relative, slash-separated, e.g. "blog/post/index.html"
	attr  string
	value string
}

// websiteLinkAttrs maps each element to the attribute holding the URL it loads or links to.
var websiteLinkAttrs = map[string]string{"a": "href", "link": "href", "img": "src", "script": "src", "source": "src"}

func scanWebsiteDeadLinks(distDir string) (websiteDeadLinkScan, error) {
	idsByPage := map[string]map[string]bool{}
	var links []websitePageLink

	err := filepath.WalkDir(distDir, func(p string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() || !strings.HasSuffix(p, ".html") {
			return err
		}
		rel, err := filepath.Rel(distDir, p)
		if err != nil {
			return err
		}
		page := filepath.ToSlash(rel)
		ids, pageLinks, err := parseWebsitePage(p, page)
		if err != nil {
			return err
		}
		idsByPage[page] = ids
		links = append(links, pageLinks...)
		return nil
	})
	if err != nil {
		return websiteDeadLinkScan{}, fmt.Errorf("walking %s: %w", distDir, err)
	}

	scan := websiteDeadLinkScan{pages: len(idsByPage)}
	seen := map[string]bool{}
	for _, link := range links {
		reason, checked := websiteLinkProblem(distDir, idsByPage, link)
		if !checked {
			continue
		}
		scan.links++
		if reason == "" {
			continue
		}
		finding := fmt.Sprintf("%s: %s=%q (%s)", link.page, link.attr, link.value, reason)
		if !seen[finding] {
			seen[finding] = true
			scan.broken = append(scan.broken, finding)
		}
	}
	sort.Strings(scan.broken)
	return scan, nil
}

// parseWebsitePage returns every `id` (and legacy `<a name>`) on the page, and its link attributes.
func parseWebsitePage(filePath, page string) (map[string]bool, []websitePageLink, error) {
	f, err := os.Open(filePath)
	if err != nil {
		return nil, nil, err
	}
	defer f.Close()
	doc, err := html.Parse(f)
	if err != nil {
		return nil, nil, fmt.Errorf("parsing %s: %w", page, err)
	}

	ids := map[string]bool{}
	var links []websitePageLink
	var walk func(n *html.Node)
	walk = func(n *html.Node) {
		if n.Type == html.ElementNode {
			linkAttr := websiteLinkAttrs[n.Data]
			for _, a := range n.Attr {
				switch {
				case a.Key == "id", n.Data == "a" && a.Key == "name":
					ids[a.Val] = true
				case a.Key == linkAttr:
					links = append(links, websitePageLink{page: page, attr: a.Key, value: a.Val})
				}
			}
		}
		for c := n.FirstChild; c != nil; c = c.NextSibling {
			walk(c)
		}
	}
	walk(doc)
	return ids, links, nil
}

// websiteLinkProblem resolves one link. checked is false for links this check doesn't judge
// (external URLs, other schemes, a bare "#"); reason is empty when the link resolves.
func websiteLinkProblem(distDir string, idsByPage map[string]map[string]bool, link websitePageLink) (reason string, checked bool) {
	value := strings.TrimSpace(link.value)
	if value == "" {
		return "empty link", true
	}
	if value == "#" {
		return "", false
	}
	u, err := url.Parse(value)
	if err != nil {
		return "unparseable URL", true
	}
	if u.Scheme != "" || u.Host != "" {
		return "", false
	}

	target := link.page
	if u.Path != "" {
		target, reason = websiteResolvePath(distDir, link.page, u.Path)
		if reason != "" {
			return reason, true
		}
	}

	fragment := u.Fragment
	if fragment == "" || strings.HasPrefix(fragment, ":~:") || !strings.HasSuffix(target, ".html") {
		return "", true
	}
	if !idsByPage[target][fragment] {
		return fmt.Sprintf("no id %q on %s", fragment, target), true
	}
	return "", true
}

// websiteResolvePath maps a URL path to the dist file a static server would answer with, the way
// nginx's `try_files $uri $uri/ $uri.html` does: the file itself, a directory's index.html, or
// the `.html` sibling.
func websiteResolvePath(distDir, fromPage, urlPath string) (string, string) {
	resolved := urlPath
	if !strings.HasPrefix(urlPath, "/") {
		resolved = path.Join("/", path.Dir(fromPage), urlPath)
		if strings.HasSuffix(urlPath, "/") {
			resolved += "/"
		}
	}
	clean := strings.TrimPrefix(path.Clean(resolved), "/")
	candidates := []string{clean, path.Join(clean, "index.html"), clean + ".html"}
	if clean == "" || clean == "." {
		candidates = []string{"index.html"}
	}
	for _, c := range candidates {
		if info, err := os.Stat(filepath.Join(distDir, filepath.FromSlash(c))); err == nil && !info.IsDir() {
			return c, ""
		}
	}
	return "", "no such page or file in dist/"
}
