#!/bin/zsh
# Builds rust/target/Tracker.app containing tracker (window), tracker-headless and tracker-mcp.
# Ad-hoc signed for local use; not notarized.
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p tracker-ui -p tracker-app -p tracker-mcp
APP="$PWD/target/Tracker.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
for bin in tracker tracker-headless tracker-mcp; do
  cp "target/release/$bin" "$APP/Contents/MacOS/$bin"
done
VERSION=$(cargo metadata --no-deps --format-version 1 | python3 -c "import json,sys; print(next(p['version'] for p in json.load(sys.stdin)['packages'] if p['name']=='tracker-ui'))")
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>tracker</string>
<key>CFBundleIdentifier</key><string>com.imakas.tracker.rs</string>
<key>CFBundleName</key><string>Tracker</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$VERSION</string>
<key>LSMinimumSystemVersion</key><string>13.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
codesign --force --sign - "$APP/Contents/MacOS/tracker-mcp" "$APP/Contents/MacOS/tracker-headless"
codesign --force --sign - "$APP"
print "Built $APP"
print "MCP command: $APP/Contents/MacOS/tracker-mcp"
