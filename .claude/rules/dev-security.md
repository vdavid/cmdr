# Dev-mode content security

When adding code that loads remote content (`fetch`, `iframe`), ask whether to disable it in dev mode:
`withGlobalTauri: true` is on in dev, which makes remote content a security risk.
