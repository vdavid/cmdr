# Privacy requests (access, export, deletion, correction)

How to handle a person's GDPR request about the data Cmdr's company holds on them. The privacy policy promises an answer
within 30 days, at `legal@getcmdr.com`. Requests are rare, so an agent runs each one by hand with David; this guide is
the process, not a feature.

## The source of truth for what we hold

The privacy policy (`apps/website/src/pages/privacy-policy.astro`) lists every kind of data we keep, where, and for how
long. ❌ Don't work from a list copied here: read the policy, then `apps/api-server/DETAILS.md` § Data retention for
where each item lives. If you find data the policy doesn't list, that's a bug in the policy: tell David.

## Steps

1. **Confirm who's asking.** Answer only the email address the data is tied to, from that same address. If the request
   comes from elsewhere, ask the person to write from the address on record. Never send data to a third address.
2. **Record it** as a private issue in `getcmdr/cmdr-reports` with the date received and the 30-day deadline. ❌ No name
   or email in the title or body; put identifying details only in a comment.
3. **Find everything tied to that email**, store by store, following the policy: license and purchase records, report
   intake (crash, error, feedback, with their attachments and the reports-board comments), newsletter and beta lists,
   and the support mailbox. Usage stats and website analytics use random ids that aren't linked to an email, so they
   usually can't be found; say so in the answer rather than guessing.
4. **Do what was asked:**
   - **Access or export**: collect each store's records as JSON (one file per store, plus a short README saying what
     each is) and send them to the person. Leave out other people's data.
   - **Deletion**: delete or anonymize each record. Keep only what a law requires, and say what and why: purchase
     records stay with Paddle for seven years under Swedish bookkeeping law. Warn first that deleting the license record
     ends their license's server checks, and ask if they want that.
   - **Correction**: fix the record in place.
   - **Objection or restriction**: stop the processing named, and say what that turns off.
5. **Payments**: Paddle is the merchant of record and a controller in its own right. Point the person to Paddle for data
   Paddle holds, and handle only our copy.
6. **Answer within 30 days**, in plain words: what we found, what we did, and what we kept and why. If it's complex and
   needs longer (the GDPR allows two more months), tell them before the 30 days run out.
7. **Close the issue** with a dated note of what was done, without the personal data.

## Requests about the app on someone's Mac

Cmdr keeps its local data on the user's Mac, and we can't see it. Point them to the removal steps at
[getcmdr.com/trust#remove](https://getcmdr.com/trust#remove), and to Settings for the per-feature deletes.
