Before wrapping up, run through this checklist:

1. Looking back at this work, will this be convenient to maintain later?
2. Will this lead to superb UX for the end-user, with sufficient transparency into the work that's happening?
3. Is this as fast as possible, adhering to the "blazing fast" promise we have?
4. Discuss with the user anything that's not great, or fix if straightforward, then repeat from point 1.
5. If you added a new Tauri command or IPC call that touches the filesystem, check `docs/architecture.md` § Platform
   constraints.
6. For every directory you touched that has a `CLAUDE.md`: re-read it, verify it still matches the code, and update any
   `Decision/Why` or `Gotcha/Why` entries your changes invalidated. Updating the doc is as important as the code change
   itself.
7. Did you have to reverse-engineer a flow, state machine, or async lifecycle that isn't documented? If so, add a brief
   section (5-10 lines) to the nearest `CLAUDE.md` so the next agent doesn't have to re-discover it.
