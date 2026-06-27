-- `referer` and `user_agent`: the HTTP request metadata of the `/download` hit, captured to illuminate
-- the large "(none)" channel bucket (see migration 0009). The website button forwards a `?ref=`, so
-- website downloads attribute cleanly; but a DIRECT hit to `/download` (a link to api.getcmdr.com shared
-- on AlternativeTo, a directory, GitHub, Reddit, a forum) carries no `?ref=` yet still arrives with an
-- HTTP `Referer` header naming the page that linked it. That header is the only first-party signal for
-- where those downloads came from, so we store its host.
--
-- `referer`: the sanitized HOST of the `Referer` header (hostname only, no path or query, so a referring
-- page's own query string never lands here), lowercased, `[a-z0-9.-]`, leading `www.` stripped, capped at
-- 120 chars. NULL when the header is absent (typed URL, privacy browser, referrer-policy strip) or invalid.
-- `user_agent`: the raw User-Agent of the hit, capped at 400 chars. Lets us separate a human browser from
-- `curl`/Homebrew/CI tooling inside the "other" source bucket. No persistent identifier: like the rest of
-- the downloads row it sits next to a daily-rotating hashed IP, so nothing here links across days.
ALTER TABLE downloads ADD COLUMN referer TEXT;
ALTER TABLE downloads ADD COLUMN user_agent TEXT;
