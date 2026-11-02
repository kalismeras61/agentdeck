#!/bin/zsh
# AgentDeck release build for other people's Macs: one disk image that runs on Apple Silicon
# and Intel, signed with your Developer ID, notarized by Apple and stapled, plus the signed
# update files AgentDeck's updater reads. Put it next to the agentdeck-m*.zip files and
# double-click it.
#
# Secrets never pass through this script's files: the signing certificate and the
# notarization password stay in your login keychain, the update key in ~/.tauri.
cd "$(dirname "$0")"
LOG="$PWD/agentdeck-release.log"
exec > >(tee "$LOG") 2>&1
set -e
echo "== AgentDeck release build $(date) =="
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
if [ -s "$HOME/.nvm/nvm.sh" ]; then . "$HOME/.nvm/nvm.sh"; fi
PROFILE="agentdeck-notary"
UPDATE_KEY="$HOME/.tauri/agentdeck-updater.key"
OUT="$PWD/AgentDeck-release"

echo "-- 1/7 Developer ID certificate"
ID=$(security find-identity -v -p codesigning | sed -n 's/.*"\(Developer ID Application: .*\)"$/\1/p' | head -1)
if [ -z "$ID" ]; then
  echo
  echo "MISSING: a \"Developer ID Application\" certificate in your keychain."
  echo "  1. Open Xcode > Settings > Accounts, pick your Apple ID and your team."
  echo "  2. Manage Certificates… > + > Developer ID Application > Done."
  echo "  3. Double-click this script again."
  echo "(Found these signing certificates instead:)"
  security find-identity -v -p codesigning || true
  exit 1
fi
TEAM=$(echo "$ID" | sed -n 's/.*(\([A-Z0-9]\{10\}\))$/\1/p')
echo "Signing as: $ID"

echo "-- 2/7 Notarization credentials (keychain profile \"$PROFILE\")"
if ! xcrun notarytool history --keychain-profile "$PROFILE" >/dev/null 2>&1; then
  echo
  echo "MISSING: credentials for Apple's notary service, saved once in your keychain."
  echo "  1. Make an app-specific password: account.apple.com > Sign-In and Security > App-Specific Passwords."
  echo "  2. In Terminal, run (with your Apple ID email):"
  echo
  echo "     xcrun notarytool store-credentials $PROFILE --apple-id YOUR_APPLE_ID_EMAIL --team-id $TEAM"
  echo
  echo "     and paste the app-specific password when it asks. It goes into your keychain only."
  echo "  3. Double-click this script again."
  exit 1
fi
echo "Notary credentials found."

ZIP=$(ls -t agentdeck-m*.zip | head -1)
echo "-- 3/7 Source from $ZIP"
TMP=$(mktemp -d)
ditto -x -k "$ZIP" "$TMP"
mkdir -p agentdeck
rsync -a --delete --exclude node_modules --exclude 'src-tauri/target' --exclude 'src-tauri/gen' "$TMP/agentdeck/" agentdeck/
rm -rf "$TMP"
cd agentdeck
npm install --no-audit --no-fund
VERSION=$(node -p "require('./src-tauri/tauri.conf.json').version")
echo "Version $VERSION"

echo "-- 4/7 Update signing key"
if [ ! -f "$UPDATE_KEY" ]; then
  echo "Making the key AgentDeck's updater checks downloads against (once, kept in ~/.tauri)."
  mkdir -p "$HOME/.tauri"
  npx tauri signer generate --ci -w "$UPDATE_KEY"
  chmod 600 "$UPDATE_KEY"
fi
PUBKEY=$(cat "$UPDATE_KEY.pub")
# The public half is safe to share; copied next to this script so it can go into the source.
cp "$UPDATE_KEY.pub" ../agentdeck-updater.pub
export TAURI_SIGNING_PRIVATE_KEY="$(cat "$UPDATE_KEY")"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""

echo "-- 5/7 Building for Apple Silicon and Intel, signed"
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
export APPLE_SIGNING_IDENTITY="$ID"
CONF=$(node -e '
  const c = { bundle: { createUpdaterArtifacts: true }, plugins: { updater: { pubkey: process.argv[1] } } };
  if (require("fs").existsSync("dist/index.html")) c.build = { beforeBuildCommand: "" };
  console.log(JSON.stringify(c));' "$PUBKEY")
npm run tauri build -- --target universal-apple-darwin --bundles app --config "$CONF"
BUNDLE="src-tauri/target/universal-apple-darwin/release/bundle/macos"
APP="$BUNDLE/AgentDeck.app"
[ -d "$APP" ] || { echo "BUILD FAILED: $APP not found"; exit 1; }
echo "Architectures: $(lipo -archs "$APP/Contents/MacOS/agentdeck")"
codesign --verify --deep --strict "$APP"

echo "-- 6/7 Notarizing (Apple usually answers in a few minutes)"
rm -rf "$OUT"; mkdir -p "$OUT"
DMG="$OUT/AgentDeck-$VERSION.dmg"
STAGE=$(mktemp -d)
ditto "$APP" "$STAGE/AgentDeck.app"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "AgentDeck" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"
codesign --sign "$ID" --timestamp "$DMG"
xcrun notarytool submit "$DMG" --keychain-profile "$PROFILE" --wait
xcrun stapler staple "$DMG"

echo "-- 7/7 Update files"
# Made and signed by the build above; the notarized disk image registered this same app with Apple.
TGZ="$OUT/AgentDeck.app.tar.gz"
cp "$BUNDLE/AgentDeck.app.tar.gz" "$TGZ"
cp "$BUNDLE/AgentDeck.app.tar.gz.sig" "$TGZ.sig"
SIG=$(cat "$TGZ.sig")
node -e '
  const [version, sig, out] = process.argv.slice(1);
  const url = `https://github.com/kalismeras61/agentdeck/releases/download/v${version}/AgentDeck.app.tar.gz`;
  const entry = { signature: sig, url };
  const json = { version, notes: `AgentDeck ${version}`, pub_date: new Date().toISOString(),
    platforms: { "darwin-aarch64": entry, "darwin-x86_64": entry, "darwin-universal": entry } };
  require("fs").writeFileSync(out, JSON.stringify(json, null, 2));' "$VERSION" "$SIG" "$OUT/latest.json"

echo "-- Checking what Gatekeeper will say"
spctl -a -t exec -vv "$APP" 2>&1 | head -3
spctl -a -t open --context context:primary-signature -vv "$DMG" 2>&1 | head -3

echo
echo "READY in $OUT:"
ls -la "$OUT"
echo "Publish: upload all three files to a GitHub release tagged v$VERSION in kalismeras61/agentdeck."
if command -v gh >/dev/null && gh auth status >/dev/null 2>&1; then
  if gh release view "v$VERSION" -R kalismeras61/agentdeck >/dev/null 2>&1; then
    echo "(Release v$VERSION already exists there; not touching it.)"
  elif gh repo view kalismeras61/agentdeck >/dev/null 2>&1; then
    gh release create "v$VERSION" -R kalismeras61/agentdeck --title "AgentDeck $VERSION" --notes "AgentDeck $VERSION" \
      "$DMG" "$TGZ" "$TGZ.sig" "$OUT/latest.json"
    echo "PUBLISHED: https://github.com/kalismeras61/agentdeck/releases/tag/v$VERSION"
  fi
fi
echo "== DONE =="
