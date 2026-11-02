# Releasing AgentDeck

AgentDeck ships as one disk image for every Mac (Apple Silicon and Intel). It is signed with the Developer ID of Yasin İlhan, notarized by Apple and stapled, so it opens with a double-click. The source, the releases and the update feed all live in the public `kalismeras61/agentdeck` repo.

## One-time setup

| What | Where it lives | How |
|---|---|---|
| Developer ID Application certificate | Your login keychain | Xcode → Settings → Accounts → your team → Manage Certificates → + → Developer ID Application |
| Notary credentials | Your login keychain, profile `agentdeck-notary` | `xcrun notarytool store-credentials agentdeck-notary --apple-id YOU@EXAMPLE.COM --team-id TEAMID`, then paste an app-specific password from account.apple.com |
| Update signing key | `~/.tauri/agentdeck-updater.key` (private), `.pub` (public) | Made by `release-mac.command` on its first run. Back the private key up: losing it means installed copies can't update to builds signed with a new key |
| Repo | GitHub | `kalismeras61/agentdeck`, public |

## Releasing (automatic)

1. Raise `version` in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` (and `Cargo.lock`) and `package.json`.
2. Optionally write the notes in `docs/releases/v<version>.md`.
3. Push to `main`.

`.github/workflows/release.yml` sees a version on `main` that has no published release yet, runs the checks, builds the universal app on GitHub's macOS machines, signs it, has Apple notarize the disk image, and publishes release `v<version>` with:

- `AgentDeck-<version>.dmg`: what people download
- `AgentDeck.app.tar.gz` and `.sig`: what installed copies download and verify
- `latest.json`: what installed copies check

The release stays a draft until the notarized disk image is attached, so installed copies never see a half-finished release. Pushes that don't change the version do nothing. A `v*` tag push or "Run workflow" in the Actions tab also starts it.

### One-time setup of the secrets

Double-click `setup-release-secrets.command` on the Mac with the certificate. It signs in the GitHub CLI if needed (browser approval), exports the certificate (macOS asks for your login password), asks for your Apple ID and a new app-specific password and checks them with Apple, stores everything as Actions secrets, and starts the first release. Nothing secret is written to disk.

| Secret | What |
|---|---|
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD` | The Developer ID certificate as .p12 (base64) and a random export password |
| `APPLE_SIGNING_IDENTITY`, `APPLE_TEAM_ID` | `Developer ID Application: … (TEAMID)` and the team ID |
| `KEYCHAIN_PASSWORD` | Random; for the build machine's temporary keychain |
| `APPLE_ID`, `APPLE_PASSWORD` | Apple ID email and an app-specific password, for notarization |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | The update signing key (empty password) |

Publishing uses the workflow's own `GITHUB_TOKEN`. The public update key is in `plugins.updater.pubkey` in `tauri.conf.json`.

## Releasing from your Mac (fallback)

`release-mac.command` (next to the `agentdeck-m*.zip` files) does the same on your Mac with the keychain's certificate and the `agentdeck-notary` profile, writing the four files to `AgentDeck-release/`, and publishes them if the GitHub CLI is signed in.

## How updating works

AgentDeck checks `https://github.com/kalismeras61/agentdeck/releases/latest/download/latest.json` quietly 8 seconds after it starts and every six hours. When a newer version is there, a notice above Settings offers **Install and restart** (it says how many working agents a restart stops). The download is checked against the public key built into the app, including the version it was signed for, so an old or tampered build isn't installed. Settings → Updates checks on demand. Builds made with `build-agentdeck.command` have no key and say they don't update themselves.

## Antigravity

Google's Antigravity terms prohibit using third-party software to access the service, and Google has suspended accounts over it (mostly tools that reuse Antigravity's sign-in). AgentDeck runs Google's own `agy` unmodified and never touches its sign-in, but whether that's allowed isn't stated. So Antigravity is **off by default**: Settings → Providers turns it on after the user ticks that they understand the risk to their Google account. People who already had Antigravity sessions keep it on. Don't advertise Antigravity support on the download page.
