// Preview server for Listmonk email templates.
// Renders the actual Go templates with sample data so you can iterate
// without deploying.
//
// Usage: `cd infra/listmonk/preview && go run .`
// Then open http://localhost:9900
package main

import (
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"text/template"
)

const port = "9900"

// ---------------------------------------------------------------------------
// Localizer mock — same interface as listmonk's L template object
// ---------------------------------------------------------------------------

type localizer struct{ t map[string]string }

func (l *localizer) T(key string) string {
	if v, ok := l.t[key]; ok {
		return v
	}
	return "[" + key + "]"
}

func (l *localizer) Ts(key string) string { return l.T(key) }

var loc = &localizer{t: map[string]string{
	"email.optin.confirmSubTitle":   "Confirm subscription",
	"email.optin.confirmSubWelcome": "Hi",
	"email.optin.confirmSubInfo":    "You have been added to the following lists:",
	"email.optin.confirmSubHelp":    "Confirm your subscription by clicking the button below.",
	"email.optin.confirmSub":        "Confirm subscription",
	"email.optin.privateList":       "Private list",
	"email.unsub":                   "Unsubscribe",
	"email.viewInBrowser":           "View in browser",
}}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

func main() {
	mux := http.NewServeMux()
	mux.HandleFunc("/", handleIndex)
	mux.HandleFunc("/optin", handleOptin)
	mux.HandleFunc("/campaign", handleCampaign)

	fmt.Printf("Email template preview → http://localhost:%s\n", port)
	if err := http.ListenAndServe(":"+port, mux); err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "error: %v\n", err)
		os.Exit(1)
	}
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

func handleIndex(w http.ResponseWriter, r *http.Request) {
	if r.URL.Path != "/" {
		http.NotFound(w, r)
		return
	}
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	_, err := fmt.Fprint(w, `<!doctype html>
<html><head><title>Email template preview</title>
<style>
body { font-family: system-ui, sans-serif; max-width: 480px; margin: 60px auto; color: #333; }
a { color: #0055d4; }
h1 { font-size: 20px; font-weight: 600; }
ul { line-height: 2; }
code { background: #f0f0f0; padding: 2px 6px; border-radius: 3px; font-size: 13px; }
.hint { color: #888; font-size: 13px; margin-top: 32px; }
</style></head><body>
<h1>Email template preview</h1>
<ul>
<li><a href="/optin">Opt-in confirmation</a> — <code>email-templates/subscriber-optin.html</code></li>
<li><a href="/campaign">Campaign newsletter</a> — <code>campaign-template.html</code></li>
</ul>
<p class="hint">Edit the template files, then refresh the browser to see changes.</p>
</body></html>`)
	if err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "error: %v\n", err)
	}
}

func handleOptin(w http.ResponseWriter, _ *http.Request) {
	tmpl, err := parseSystemTemplates("../email-templates")
	if err != nil {
		http.Error(w, "Template parse error:\n"+err.Error(), 500)
		return
	}

	data := map[string]any{
		"L": loc,
		"Subscriber": map[string]string{
			"FirstName": "Alex",
			"Email":     "alex@example.com",
			"Name":      "Alex Johnson",
		},
		"Lists": []map[string]string{
			{"Name": "Cmdr newsletter", "Type": "public"},
			{"Name": "Beta testers", "Type": "private"},
		},
		"OptinURL": "#confirm",
		"UnsubURL": "#unsubscribe",
	}

	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	if err := tmpl.ExecuteTemplate(w, "subscriber-optin", data); err != nil {
		http.Error(w, "Render error:\n"+err.Error(), 500)
	}
}

func handleCampaign(w http.ResponseWriter, _ *http.Request) {
	tmpl, err := parseCampaignTemplate("../campaign-template.html")
	if err != nil {
		http.Error(w, "Template parse error:\n"+err.Error(), 500)
		return
	}

	data := map[string]any{
		"L": loc,
		"Campaign": map[string]any{
			"Subject": "What's new in Cmdr — February 2026",
			"Attribs": map[string]string{
				"preheader": "Fresh features, bug fixes, and what's coming next.",
			},
		},
	}

	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	if err := tmpl.Execute(w, data); err != nil {
		http.Error(w, "Render error:\n"+err.Error(), 500)
	}
}

// ---------------------------------------------------------------------------
// Template loading
// ---------------------------------------------------------------------------

// parseSystemTemplates loads email-templates/*.html (base + individual emails).
// Templates are re-parsed on every request so edits show up on browser refresh.
func parseSystemTemplates(dir string) (*template.Template, error) {
	files, err := filepath.Glob(filepath.Join(dir, "*.html"))
	if err != nil {
		return nil, err
	}
	if len(files) == 0 {
		return nil, fmt.Errorf("no .html files in %s", dir)
	}

	tmpl := template.New("root")
	for _, f := range files {
		raw, err := os.ReadFile(f)
		if err != nil {
			return nil, err
		}
		src := rewriteLocalizerCalls(string(raw))
		if _, err := tmpl.Parse(src); err != nil {
			return nil, fmt.Errorf("%s: %w", filepath.Base(f), err)
		}
	}
	return tmpl, nil
}

// parseCampaignTemplate loads campaign-template.html and injects a sample
// "content" block so the template renders end-to-end.
func parseCampaignTemplate(file string) (*template.Template, error) {
	raw, err := os.ReadFile(file)
	if err != nil {
		return nil, err
	}

	src := rewriteLocalizerCalls(string(raw))
	funcMap := template.FuncMap{
		"UnsubscribeURL": func() string { return "#unsubscribe" },
		"MessageURL":     func() string { return "#message" },
		"TrackView":      func() string { return "" },
	}

	tmpl, err := template.New("campaign").Funcs(funcMap).Parse(src)
	if err != nil {
		return nil, err
	}
	if _, err := tmpl.New("content").Parse(sampleCampaignContent); err != nil {
		return nil, err
	}
	return tmpl, nil
}

// rewriteLocalizerCalls turns `{{ L.Method` into `{{ $.L.Method` so Go's
// template engine resolves the localizer from the root data map. We use `$`
// (not `.`) because `.` gets rebound inside {{ range }} blocks.
func rewriteLocalizerCalls(src string) string {
	return strings.ReplaceAll(src, "{{ L.", "{{ $.L.")
}

// ---------------------------------------------------------------------------
// Sample content — exercises most CSS styles in the campaign template
// ---------------------------------------------------------------------------

const sampleCampaignContent = `
<h1>What's new in Cmdr</h1>
<p>Hey Alex,</p>
<p>Here's what we've been up to this month. Cmdr keeps getting faster, smarter,
and more keyboard-friendly.</p>

<h2>Highlights</h2>
<ul>
<li><strong>Batch rename</strong> — rename dozens of files with a single pattern.
<a href="https://getcmdr.com">Learn more</a></li>
<li><strong>Quick preview</strong> — press Space to preview any file without
leaving the file list</li>
<li><strong>Faster SMB</strong> — network folder loading is now 3x faster on
large shares</li>
</ul>

<blockquote>
"I switched from Forklift and haven't looked back. The speed difference is
insane."<br>— A happy Cmdr user
</blockquote>

<h2>Try it out</h2>
<p>Update to the latest version to get all these improvements:</p>
<p><a href="https://getcmdr.com" class="button">Download Cmdr</a></p>

<hr>

<h3>A bit of code</h3>
<pre>cmdr --version
Cmdr 2.4.0 (build 1337)</pre>

<h3>Coming next</h3>
<p>We're working on <strong>tabs</strong>, <strong>bookmarks</strong>, and a
built-in <code>terminal</code> panel. Stay tuned!</p>

<p>Happy file managing,<br>The Cmdr team</p>
`
