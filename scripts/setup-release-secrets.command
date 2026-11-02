#!/bin/zsh
# Puts what GitHub Actions needs to sign and notarize AgentDeck into the kalismeras61/agentdeck
# repo's Actions secrets. Run once on your Mac, in Terminal.
#
# Every password is typed by you at a prompt (nothing is echoed or written to disk except a
# temporary certificate export that's deleted at the end). Needs the GitHub CLI signed in
# (`brew install gh && gh auth login`) and has to run after release-mac.command has made the
# update key.
set -e
REPO="kalismeras61/agentdeck"
UPDATE_KEY="$HOME/.tauri/agentdeck-updater.key"
command -v gh >/dev/null || { echo "Install the GitHub CLI first: brew install gh && gh auth login"; exit 1; }
gh auth status >/dev/null || { echo "Sign the GitHub CLI in first: gh auth login"; exit 1; }
[ -f "$UPDATE_KEY" ] || { echo "Run release-mac.command once first; it makes the update key."; exit 1; }

ID=$(security find-identity -v -p codesigning | sed -n 's/.*"\(Developer ID Application: .*\)"$/\1/p' | head -1)
[ -n "$ID" ] || { echo "No Developer ID Application certificate in your keychain (see release-mac.command)."; exit 1; }
TEAM=$(echo "$ID" | sed -n 's/.*(\([A-Z0-9]\{10\}\))$/\1/p')
echo "Certificate: $ID"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
echo
echo "1/3 Exporting the certificate. Choose a password for the export file when asked;"
echo "    macOS may also ask for your login keychain password to allow the export."
read -rs "P12PASS?Password for the export (not stored on disk): "; echo
security export -k login.keychain-db -t identities -f pkcs12 -P "$P12PASS" -o "$TMP/cert.p12"
# The export holds every signing identity in the keychain; the build picks the Developer ID one by name.
base64 -i "$TMP/cert.p12" | gh secret set APPLE_CERTIFICATE -R "$REPO"
printf %s "$P12PASS" | gh secret set APPLE_CERTIFICATE_PASSWORD -R "$REPO"
printf %s "$ID" | gh secret set APPLE_SIGNING_IDENTITY -R "$REPO"
printf %s "$TEAM" | gh secret set APPLE_TEAM_ID -R "$REPO"
openssl rand -base64 24 | gh secret set KEYCHAIN_PASSWORD -R "$REPO"
unset P12PASS

echo
echo "2/3 Notarization: your Apple ID email and an app-specific password"
echo "    (account.apple.com > Sign-In and Security > App-Specific Passwords)."
read -r "AID?Apple ID email: "
printf %s "$AID" | gh secret set APPLE_ID -R "$REPO"
read -rs "APW?App-specific password: "; echo
printf %s "$APW" | gh secret set APPLE_PASSWORD -R "$REPO"
unset APW

echo
echo "3/3 Update signing key (from $UPDATE_KEY)"
gh secret set TAURI_SIGNING_PRIVATE_KEY -R "$REPO" < "$UPDATE_KEY"
printf %s "" | gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD -R "$REPO"

echo
echo "Done. Secrets now in $REPO:"
gh secret list -R "$REPO"
