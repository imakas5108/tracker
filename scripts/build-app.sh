#!/bin/zsh
set -eu
cd "$(dirname "$0")/.."
export CLANG_MODULE_CACHE_PATH="$PWD/.build/ModuleCache"
swift build -c release -Xswiftc -swift-version -Xswiftc 5
BIN_DIR="$(swift build -c release --show-bin-path)"
APP="$PWD/build/Tracker.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp THIRD_PARTY_NOTICES.txt "$APP/Contents/Resources/THIRD_PARTY_NOTICES.txt"
cp "$BIN_DIR/Tracker" "$APP/Contents/MacOS/Tracker"
cp "$BIN_DIR/TrackerMCP" "$APP/Contents/MacOS/TrackerMCP"
codesign --force --sign - "$APP/Contents/MacOS/TrackerMCP"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>Tracker</string>
<key>CFBundleIdentifier</key><string>com.imakas.tracker</string>
<key>CFBundleName</key><string>Tracker</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.3.0</string>
<key>LSMinimumSystemVersion</key><string>14.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
codesign --force --sign - "$APP"
print "Built $APP"
