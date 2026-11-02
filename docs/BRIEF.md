# AgentDeck — working brief (Claude edition)

Adapted from the original GrokBot brief. Product, stack, design tokens, security rules and the definition of done are unchanged; this file records what changed in *how* the work is run.

## Owner

Claude is the sole engineering owner and does the implementation itself. Claude Code and Codex are products AgentDeck integrates, not collaborators.

## How the work is run

1. **Milestone by milestone, each ending in a working demo.** No milestone starts until the previous one has been run, screenshotted and reviewed.
2. **Phase 0 is folded into milestone 1:** official provider docs are checked before any adapter code is written (see `PROVIDERS.md`).
3. **Evidence over claims.** Every "done" item in `STATUS.md` names the command or screenshot that proves it.
4. **Honest states.** Anything not built yet shows an explicit "not yet" state, never a fake.

## Milestones (re-cut from the original five phases)

| # | Scope | Demo at the end |
|---|-------|-----------------|
| 1 | Tauri + React shell, flagship **Workspace** screen with labeled demo data, normalized event schema + validation, theme/motion, shortcuts, resize/collapse | Running app screenshots at 1440×900 and 1280×800 |
| 2 | Remaining screens on demo data: Session overview, Diff review (Monaco), Permission center, Rich output gallery, full Settings | Screenshots of each screen, keyboard-only walkthrough |
| 3 | SQLite event store, session CRUD, restart recovery, Claude Code adapter (unmodified `claude` CLI, stream-json, permission routing) | Real session against a sample Flutter project, killed and restored |
| 4 | Codex adapter via the Codex App Server (JSON-RPC over stdio), capability matrix filled from real runs | Two providers running side by side |
| 5 | PTY + xterm.js terminal, performance, accessibility pass, diagnostics, signed macOS build | Installable `.dmg` |

The original brief put all seven polished screens in phase 1. That was split: milestone 1 makes the flagship screen excellent, milestone 2 completes the rest on the same components.

## Measurable visual checks (replace "premium")

- Text contrast ≥ 4.5:1 for body and secondary text in both themes
- Hit targets ≥ 28×28px in the dense chrome, ≥ 32×32px elsewhere
- No horizontal page overflow at 1000px window width (the app's minimum)
- Conversation text column ≤ 72ch
- Reduced motion: no looping animation, no animated transitions
- Every interactive control reachable and operable by keyboard with a visible focus ring
