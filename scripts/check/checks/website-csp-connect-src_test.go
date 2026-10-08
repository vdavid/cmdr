package checks

import (
	"reflect"
	"strings"
	"testing"
)

func TestCspConnectSourcesReadsConnectSrc(t *testing.T) {
	conf := `add_header X-Frame-Options "SAMEORIGIN" always;
add_header Content-Security-Policy "default-src 'self'; connect-src 'self' https://api.example.com https://*.paddle.com; frame-src 'none'" always;`
	got, err := cspConnectSources(conf)
	if err != nil {
		t.Fatal(err)
	}
	want := []string{"'self'", "https://api.example.com", "https://*.paddle.com"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %v, want %v", got, want)
	}
}

func TestCspConnectSourcesFallsBackToDefaultSrc(t *testing.T) {
	got, err := cspConnectSources(`add_header Content-Security-Policy "default-src 'self'" always;`)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(got, []string{"'self'"}) {
		t.Fatalf("got %v", got)
	}
}

func TestCspConnectSourcesRejectsDuplicatedHeader(t *testing.T) {
	// Two copies is how the CSP drifted apart before; the snippet must hold exactly one.
	conf := strings.Repeat(`add_header Content-Security-Policy "default-src 'self'" always;`+"\n", 2)
	if _, err := cspConnectSources(conf); err == nil {
		t.Fatal("expected an error for two CSP headers")
	}
}

func TestCspAllows(t *testing.T) {
	sources := []string{"'self'", "https://api.getcmdr.com", "https://*.paddle.com"}
	cases := []struct {
		origin string
		want   bool
	}{
		{"", true},
		{"https://api.getcmdr.com", true},
		{"https://checkout.paddle.com", true},
		{"https://paddle.com", false},
		{"https://evilpaddle.com", false},
		{"http://api.getcmdr.com", false},
		{"https://comments.getcmdr.com", false},
	}
	for _, c := range cases {
		if got := cspAllows(sources, c.origin); got != c.want {
			t.Errorf("cspAllows(%q) = %v, want %v", c.origin, got, c.want)
		}
	}
	if cspAllows([]string{"https://api.getcmdr.com"}, "") {
		t.Error("a relative URL needs 'self'")
	}
}

func TestFindFetchTargets(t *testing.T) {
	src := `
const apiBase = 'https://api.getcmdr.com'
fetch(` + "`${apiBase}/likes/${encodeURIComponent(slug)}`" + `)
fetch('https://api.getcmdr.com/r-codes.json')
fetch('/api/newsletter/subscribe', { method: 'POST' })
navigator.sendBeacon("https://Stats.Example.com/x", body)
fetch(someUrl)
`
	got := findFetchTargets(src)
	want := []fetchTarget{
		{line: 3, origin: "https://api.getcmdr.com"},
		{line: 4, origin: "https://api.getcmdr.com"},
		{line: 5, origin: ""},
		{line: 6, origin: "https://stats.example.com"},
		{line: 7, unresolved: "someUrl"},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %+v\nwant %+v", got, want)
	}
}
