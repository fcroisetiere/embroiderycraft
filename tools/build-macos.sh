#!/usr/bin/env bash
# Build EmbroideryCraft.app for macOS (Apple Silicon and Intel in one app) and zip it in dist/.
# Run it on a Mac, from the project folder:   tools/build-macos.sh
# Needs the Xcode command line tools (the script asks macOS to install them if missing) and Rust
# (installed for this user with rustup if missing).
set -euo pipefail
cd "$(dirname "$0")/.."

if ! xcode-select -p >/dev/null 2>&1; then
    echo "Installing the Xcode command line tools: accept the dialog, then run this script again."
    xcode-select --install || true
    exit 1
fi
if ! command -v cargo >/dev/null 2>&1; then
    [ -x "${HOME}/.cargo/bin/cargo" ] || curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    export PATH="${HOME}/.cargo/bin:${PATH}"
fi
rustup target add aarch64-apple-darwin x86_64-apple-darwin

export MACOSX_DEPLOYMENT_TARGET=11.0
for t in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --release -p embcraft-gui -p embcraft --target "$t"
done

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
OUT="dist/EmbroideryCraft-${VERSION}-macos"
APP="${OUT}/EmbroideryCraft.app"
rm -rf "${OUT}" "${OUT}.zip"
mkdir -p "${APP}/Contents/MacOS" "${APP}/Contents/Resources"
lipo -create -output "${APP}/Contents/MacOS/EmbroideryCraft" \
    target/aarch64-apple-darwin/release/embcraft-gui target/x86_64-apple-darwin/release/embcraft-gui
lipo -create -output "${OUT}/embcraft" \
    target/aarch64-apple-darwin/release/embcraft target/x86_64-apple-darwin/release/embcraft
cat > "${APP}/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>EmbroideryCraft</string>
    <key>CFBundleDisplayName</key><string>EmbroideryCraft</string>
    <key>CFBundleIdentifier</key><string>org.embroiderycraft.editor</string>
    <key>CFBundleExecutable</key><string>EmbroideryCraft</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>${VERSION}</string>
    <key>CFBundleVersion</key><string>${VERSION}</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.graphics-design</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict>
</plist>
PLIST
# ad-hoc signature: required to run on Apple Silicon (not a Developer ID, see docs/MACOS.md)
codesign --force --deep --sign - "${APP}"
cp docs/MACOS.md "${OUT}/README-macOS.md"
cp LICENSE* "${OUT}/" 2>/dev/null || true
ditto -c -k --keepParent "${OUT}" "${OUT}.zip"
echo "Built ${OUT}.zip  (the app: ${APP})"
