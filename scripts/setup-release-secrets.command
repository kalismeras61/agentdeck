#!/bin/zsh
# One-time setup for automatic releases. Double-click it on the Mac that has the Developer ID
# certificate. It gives the kalismeras61/agentdeck repo's GitHub Actions what they need to sign,
# notarize and publish AgentDeck, then starts the first automatic release. After this, raising the
# version on main is all a release takes.
#
# What it asks you for, and nothing else:
#   - GitHub: approve the sign-in in your browser (once, if the GitHub CLI isn't signed in)
#   - macOS: your login password, to allow exporting the signing certificate
#   - Apple: your Apple ID email and an app-specific password, for notarization
# Passwords go straight from the prompt to GitHub's encrypted secrets. Nothing secret is written
# to disk or to the log next to this script.
cd "$(dirname "$0")"
LOG="$PWD/setup-release-secrets.log"
exec > >(tee "$LOG") 2>&1
set -e
export PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"
REPO="kalismeras61/agentdeck"
UPDATE_KEY="$HOME/.tauri/agentdeck-updater.key"
echo "== AgentDeck automatic releases: one-time setup $(date) =="

echo "-- 1/5 GitHub"
if ! command -v gh >/dev/null; then
  command -v brew >/dev/null || { echo "MISSING: Homebrew (https://brew.sh), to install the GitHub CLI."; exit 1; }
  echo "Installing the GitHub CLI…"
  brew install gh
fi
if ! gh auth status -h github.com >/dev/null 2>&1; then
  echo "Sign in to GitHub: press Enter, then approve in the browser with the code shown."
  gh auth login -h github.com -p https -w -s repo,workflow
fi
PERM=$(gh api "repos/$REPO" -q .permissions.admin 2>/dev/null || echo false)
[ "$PERM" = "true" ] || { echo "FAILED: the signed-in GitHub account can't change $REPO's settings. Sign in as kalismeras61 (gh auth login) and run this again."; exit 1; }
echo "Signed in to GitHub with access to $REPO."

echo "-- 2/5 Signing certificate"
ID=$(security find-identity -v -p codesigning | sed -n 's/.*"\(Developer ID Application: .*\)"$/\1/p' | head -1)
[ -n "$ID" ] || { echo "MISSING: a Developer ID Application certificate in your keychain (see docs/RELEASING.md)."; exit 1; }
TEAM=$(echo "$ID" | sed -n 's/.*(\([A-Z0-9]\{10\}\))$/\1/p')
echo "Certificate: $ID"
[ -f "$UPDATE_KEY" ] || { echo "MISSING: the update key $UPDATE_KEY (release-agentdeck.command makes it)."; exit 1; }
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
P12PASS=$(openssl rand -base64 24)
echo "macOS will ask for your login password to allow the export. Click Allow (or Always Allow)."
security export -k login.keychain-db -t identities -f pkcs12 -P "$P12PASS" -o "$TMP/cert.p12"
base64 -i "$TMP/cert.p12" | gh secret set APPLE_CERTIFICATE -R "$REPO"
rm -f "$TMP/cert.p12"
printf %s "$P12PASS" | gh secret set APPLE_CERTIFICATE_PASSWORD -R "$REPO"
unset P12PASS
printf %s "$ID" | gh secret set APPLE_SIGNING_IDENTITY -R "$REPO"
printf %s "$TEAM" | gh secret set APPLE_TEAM_ID -R "$REPO"
openssl rand -base64 24 | gh secret set KEYCHAIN_PASSWORD -R "$REPO"
echo "Certificate saved to GitHub."

echo "-- 3/5 Notarization"
echo "Apple needs an app-specific password for the build machine. Make a new one now:"
echo "  account.apple.com > Sign-In and Security > App-Specific Passwords > +  (name it: AgentDeck GitHub)"
open "https://account.apple.com/account/manage/section/security" 2>/dev/null || true
while true; do
  read -r "AID?Apple ID email: "
  read -rs "APW?App-specific password (xxxx-xxxx-xxxx-xxxx, not shown): "; echo
  if xcrun notarytool history --apple-id "$AID" --password "$APW" --team-id "$TEAM" >/dev/null 2>&1; then
    break
  fi
  echo "Apple didn't accept that email and password for team $TEAM. Try again."
done
printf %s "$AID" | gh secret set APPLE_ID -R "$REPO"
printf %s "$APW" | gh secret set APPLE_PASSWORD -R "$REPO"
unset APW
echo "Notarization works; saved to GitHub."

echo "-- 4/5 Update signing key"
gh secret set TAURI_SIGNING_PRIVATE_KEY -R "$REPO" < "$UPDATE_KEY"
printf %s "" | gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD -R "$REPO"
echo "Saved to GitHub."
gh secret list -R "$REPO"

echo "-- 5/5 First automatic release"
gh workflow run release.yml -R "$REPO" --ref main
sleep 5
gh run list -R "$REPO" --workflow release.yml --limit 1
echo
echo "SET UP. GitHub is building, signing, notarizing and publishing the current version now"
echo "(about 20 minutes): https://github.com/$REPO/actions"
echo "From now on every new version pushed to main is released the same way."
echo "== DONE =="
