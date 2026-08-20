package checks

import "testing"

// SvelteKit emits `_app/immutable/chunks/<hash>.js`, where the hash IS the
// basename, so the Astro-shaped `name.hash.ext` normalizer leaves it alone and
// the baseline's topAssets churn on every rebuild. These pin the desktop
// normalizer that closes that.
func TestNormalizeDesktopAssetName(t *testing.T) {
	cases := []struct{ in, want string }{
		// The bare-hash case this exists for; hash length varies by emitter.
		{"_app/immutable/chunks/8ndMSAZW.js", "_app/immutable/chunks/*.js"},
		{"_app/immutable/chunks/us4CX05T2.js", "_app/immutable/chunks/*.js"},
		{"_app/immutable/entry/start.DjVvJ3nX.js", "_app/immutable/entry/start.*.js"},
		// `name.hash.ext` still normalizes the Astro way.
		{"_app/immutable/nodes/5.DV2WG7G5.js", "_app/immutable/nodes/5.*.js"},
		{"_app/immutable/assets/5.BqK3mZ1p.css", "_app/immutable/assets/5.*.css"},
		// Outside `_app/immutable/` nothing is hashed, so nothing is collapsed.
		{"index.html", "index.html"},
		{"favicon.png", "favicon.png"},
		{"icons/32.png", "icons/32.png"},
	}
	for _, c := range cases {
		if got := normalizeDesktopAssetName(c.in); got != c.want {
			t.Errorf("normalizeDesktopAssetName(%q) = %q, want %q", c.in, got, c.want)
		}
	}
}

// Two different chunks must land on one identity, so the baseline records the
// directory's weight rather than a list that turns over every build.
func TestDesktopChunksCollapseToOneEntry(t *testing.T) {
	assets := map[string]int64{}
	for _, p := range []string{"_app/immutable/chunks/AbCdEfGh.js", "_app/immutable/chunks/ZyXwVuTs.js"} {
		assets[normalizeDesktopAssetName(p)] += 100
	}
	if len(assets) != 1 {
		t.Fatalf("expected one collapsed entry, got %v", assets)
	}
	if assets["_app/immutable/chunks/*.js"] != 200 {
		t.Errorf("expected the two chunks to sum, got %v", assets)
	}
}
