package main

import (
	"testing"
	"time"
)

func TestAfterGHFailure(t *testing.T) {
	now := time.Date(2026, 10, 20, 10, 49, 0, 0, time.UTC)
	spentUntil := func(reset time.Time) *apiBudget { return &apiBudget{spent: true, reset: reset} }
	inTenMin := now.Add(10 * time.Minute)
	cases := []struct {
		name     string
		budget   *apiBudget
		retries  int
		want     ghRetry
		wantWait time.Duration
	}{
		{"budget spent: wait for the reset", spentUntil(inTenMin), 2, ghWaitForReset, 10*time.Minute + resetSlack},
		{"a write with no retries still waits out a spent budget", spentUntil(inTenMin), 0, ghWaitForReset, 10*time.Minute + resetSlack},
		{"reset already passed: a short wait", spentUntil(now.Add(-time.Minute)), 2, ghWaitForReset, resetSlack},
		{"a secondary limit's Retry-After", &apiBudget{retryAfter: time.Minute}, 0, ghWaitForReset, time.Minute + resetSlack},
		{"budget left: a short retry", &apiBudget{}, 2, ghRetryTransient, transientWait},
		{"budget unknown: a short retry", nil, 1, ghRetryTransient, transientWait},
		{"out of retries", &apiBudget{}, 0, ghGiveUp, 0},
	}
	for _, c := range cases {
		got, wait := afterGHFailure(c.budget, c.retries, 0, now)
		if got != c.want || wait != c.wantWait {
			t.Errorf("%s: got %d after %s, want %d after %s", c.name, got, wait, c.want, c.wantWait)
		}
	}
}

// A secondary limit with no Retry-After backs off from a minute, doubling, even for a write (the
// call never reached GitHub's handler), and gives up once the backoffs are spent.
func TestAfterGHFailureBacksOffASecondaryLimit(t *testing.T) {
	now := time.Date(2026, 10, 21, 12, 0, 0, 0, time.UTC)
	limited := &apiBudget{limited: true}
	for backedOff, want := range []time.Duration{time.Minute, 2 * time.Minute, 4 * time.Minute, 8 * time.Minute} {
		got, wait := afterGHFailure(limited, 0, backedOff, now)
		if got != ghBackOff || wait != want {
			t.Errorf("backoff %d: got %d after %s, want %d after %s", backedOff, got, wait, ghBackOff, want)
		}
	}
	if got, _ := afterGHFailure(limited, 2, secondaryBackoffs, now); got != ghGiveUp {
		t.Errorf("backoffs spent: got %d, want %d", got, ghGiveUp)
	}
}

// A 304 answers from the cached body, which costs no budget; a 200 refreshes the cache.
func TestResponseCache(t *testing.T) {
	c := responseCache{}
	path := "repos/o/r/actions/runs?head_sha=abc"
	if etag := c.etag(path); etag != "" {
		t.Errorf("empty cache: etag %q", etag)
	}
	if body, ok := c.settle(path, apiResponse{status: 200, etag: `W/"1"`, body: "first"}); !ok || body != "first" {
		t.Errorf("200: got %q ok=%v", body, ok)
	}
	if etag := c.etag(path); etag != `W/"1"` {
		t.Errorf("after 200: etag %q", etag)
	}
	if body, ok := c.settle(path, apiResponse{status: 304, etag: `"1"`}); !ok || body != "first" {
		t.Errorf("304: got %q ok=%v, want the cached body", body, ok)
	}
	if body, ok := c.settle("repos/o/r/other", apiResponse{status: 304}); ok {
		t.Errorf("304 with nothing cached: got %q ok=%v, want a failure", body, ok)
	}
	if _, ok := c.settle(path, apiResponse{status: 403}); ok {
		t.Error("403: ok")
	}
	var none responseCache
	if etag := none.etag(path); etag != "" {
		t.Errorf("nil cache: etag %q", etag)
	}
	if body, ok := none.settle(path, apiResponse{status: 200, etag: `W/"2"`, body: "fresh"}); !ok || body != "fresh" {
		t.Errorf("nil cache, 200: got %q ok=%v", body, ok)
	}
}

// The shape `gh api --include` printed on 2026-10-20 (gh 2.x): a bare `\n` after the status line,
// `\r\n` after each header, and no Content-Length.
func TestParseAPIResponse(t *testing.T) {
	limited := "HTTP/2.0 403 Forbidden\n" +
		"Content-Type: application/json; charset=utf-8\r\n" +
		"X-Ratelimit-Limit: 5000\r\n" +
		"X-Ratelimit-Remaining: 0\r\n" +
		"X-Ratelimit-Reset: 1792493537\r\n" +
		"X-Ratelimit-Resource: core\r\n" +
		"\r\n" +
		`{"message":"API rate limit exceeded","status":"403"}`
	cases := []struct {
		name string
		out  string
		want apiResponse
	}{
		{"rate limited", limited, apiResponse{
			status: 403,
			budget: apiBudget{spent: true, reset: time.Unix(1792493537, 0)},
			body:   `{"message":"API rate limit exceeded","status":"403"}`,
		}},
		{"fine", "HTTP/2.0 200 OK\nEtag: W/\"abc\"\r\nX-Ratelimit-Remaining: 4999\r\n\r\n{\"total_count\":0}\n", apiResponse{
			status: 200, etag: `W/"abc"`, body: "{\"total_count\":0}\n",
		}},
		{"secondary", "HTTP/2.0 403 Forbidden\nRetry-After: 60\r\nX-Ratelimit-Remaining: 4000\r\n\r\n{}", apiResponse{
			status: 403, budget: apiBudget{retryAfter: time.Minute}, body: "{}",
		}},
		// GitHub's secondary limit doesn't always send Retry-After (its docs: "wait for at least
		// one minute", then back off), and the primary budget still has calls left.
		{"secondary without Retry-After", "HTTP/2.0 429 Too Many Requests\nX-Ratelimit-Remaining: 4000\r\n\r\n{}", apiResponse{
			status: 429, budget: apiBudget{limited: true}, body: "{}",
		}},
		{"not modified", "HTTP/2.0 304 Not Modified\nEtag: \"abc\"\r\nX-Ratelimit-Remaining: 4999\r\n\r\n", apiResponse{
			status: 304, etag: `"abc"`,
		}},
	}
	for _, c := range cases {
		got, ok := parseAPIResponse(c.out)
		if !ok || got.status != c.want.status || got.etag != c.want.etag || got.body != c.want.body ||
			got.budget.spent != c.want.budget.spent || !got.budget.reset.Equal(c.want.budget.reset) ||
			got.budget.retryAfter != c.want.budget.retryAfter || got.budget.limited != c.want.budget.limited {
			t.Errorf("%s: got %+v ok=%v, want %+v", c.name, got, ok, c.want)
		}
	}
	if _, ok := parseAPIResponse(""); ok {
		t.Error("no response: ok")
	}
}

func TestRateLimitBudget(t *testing.T) {
	var l rateLimit
	l.Resources.Core = rateBucket{Remaining: 0, Reset: 100}
	l.Resources.GraphQL = rateBucket{Remaining: 0, Reset: 200}
	if b := l.budget(); !b.spent || b.reset.Unix() != 200 {
		t.Errorf("both spent: got %+v, want the later reset", b)
	}
	l.Resources.Core.Remaining = 10
	l.Resources.GraphQL.Remaining = 10
	if b := l.budget(); b.spent {
		t.Errorf("budget left: got %+v", b)
	}
}
