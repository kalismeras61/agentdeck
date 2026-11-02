# Releasing AgentDeck

AgentDeck ships as one disk image for every Mac (Apple Silicon and Intel). It is signed with the Developer ID of Yasin İlhan, notarized by Apple and stapled, so it opens with a double-click. The source, the releases and the update feed all live in the public `kalismeras61/agentdeck` repo.

## One-time setup

| What | Where it lives | How |
|---|---|---|
| Developer ID Application certificate | Your login keychain | Xcode → Settings → Accounts → your team → Manage Certificates → + → Developer ID Application |
| Notary credentials | Your login keychain, profile `agentdeck-notary` | `xcrun notarytool store-credentials agentdeck-notary --apple-id YOU@EXAMPLE.COM --team-id TEAMID`, then paste an app-specific password from account.apple.com |
| Update signing key | `~/.tauri/agentdeck-updater.key` (private), `.pub` (public) | Made by `release-mac.command` on its first run. Back the private key up: losing it means installed copies can't update to builds signed with a new key |
| Repo | GitHub | `kalismeras61/agentdeck`, public |

## Release from your Mac

1. Raise `version` in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `package.json`.
2. Double-click `release-mac.command` (next to the `agentdeck-m*.zip` files). It checks the certificate and notary credentials, builds the universal app, signs it, notarizes and staples the disk image, and writes `AgentDeck-release/` with:
   - `AgentDeck-<version>.dmg`: what people download
   - `AgentDeck.app.tar.gz` and `.sig`: what installed copies download and verify
   - `latest.json`: what installed copies check
3. If the GitHub CLI is signed in, it publishes these as release `v<version>` of `kalismeras61/agentdeck`. Otherwise upload the four files to a new release with that tag yourself.

## Release from GitHub Actions

`.github/workflows/release.yml` runs the tests, then builds, signs, notarizes and publishes a release of this repo when a `v*` tag is pushed. Run `setup-release-secrets.command` once on your Mac to fill in its secrets (you type every password; nothing is written to disk):

| Secret | What |
|---|---|
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD` | The Developer ID certificate exported as .p12 (base64) and the export password |
| `APPLE_SIGNING_IDENTITY`, `APPLE_TEAM_ID` | `Developer ID Application: … (TEAMID)` and the team ID |
| `KEYCHAIN_PASSWORD` | Random; for the build machine's temporary keychain |
| `APPLE_ID`, `APPLE_PASSWORD` | Apple ID email and an app-specific password, for notarization |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | The update signing key (empty password) |

Publishing uses the workflow's own `GITHUB_TOKEN`. The public update key must be in `plugins.updater.pubkey` in `tauri.conf.json` for builds made by the workflow.

## How updating works

AgentDeck checks `https://github.com/kalismeras61/agentdeck/releases/latest/download/latest.json` quietly 8 seconds after it starts and every six hours. When a newer version is there, a notice above Settings offers **Install and restart** (it says how many working agents a restart stops). The download is checked against the public key built into the app, including the version it was signed for, so an old or tampered build isn't installed. Settings → Updates checks on demand. Builds made with `build-agentdeck.command` have no key and say they don't update themselves.

## Antigravity

Google's Antigravity terms prohibit using third-party software to access the service, and Google has suspended accounts over it (mostly tools that reuse Antigravity's sign-in). AgentDeck runs Google's own `agy` unmodified and never touches its sign-in, but whether that's allowed isn't stated. So Antigravity is **off by default**: Settings → Providers turns it on after the user ticks that they understand the risk to their Google account. People who already had Antigravity sessions keep it on. Don't advertise Antigravity support on the download page.
