#!/usr/bin/env bash
# Build the Rayview Meta macOS desktop client as a .app bundle (and optional .dmg).
#
# Usage:
#   scripts/build-macos.sh [aarch64|x86_64|universal]
#
# Run this on a macOS host (or in a macOS GitHub Actions runner). It produces:
#   dist/macos/RayviewMeta.app
#   dist/macos/RayviewMeta-<arch>.dmg   (if create-dmg is installed)

set -euo pipefail

ARCH="${1:-aarch64}"
APP_NAME="RayviewMeta"
BIN_NAME="rayview-client"
BUNDLE_ID="com.lanyao.rayview.meta"
VERSION="$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"
DIST_DIR="dist/macos"
APP_DIR="${DIST_DIR}/${APP_NAME}.app"
CONTENTS_DIR="${APP_DIR}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RES_DIR="${CONTENTS_DIR}/Resources"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: this script must be run on macOS" >&2
  exit 1
fi

case "$ARCH" in
  aarch64)
    TARGETS=("aarch64-apple-darwin")
    DMG_ARCH="arm64"
    ;;
  x86_64)
    TARGETS=("x86_64-apple-darwin")
    DMG_ARCH="x86_64"
    ;;
  universal)
    TARGETS=("aarch64-apple-darwin" "x86_64-apple-darwin")
    DMG_ARCH="universal"
    ;;
  *)
    echo "error: unknown arch '$ARCH' (expected aarch64|x86_64|universal)" >&2
    exit 1
    ;;
esac

echo "==> Building $APP_NAME v$VERSION for: ${TARGETS[*]}"

# Ensure rust targets are installed (no-op if already present).
for t in "${TARGETS[@]}"; do
  rustup target add "$t" >/dev/null 2>&1 || true
done

# Compile each target in release mode.
BIN_PATHS=()
for t in "${TARGETS[@]}"; do
  echo "==> cargo build --release --target $t"
  cargo build --release --target "$t"
  BIN_PATHS+=("target/$t/release/$BIN_NAME")
done

# Stage app bundle skeleton.
rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR" "$RES_DIR"

# Combine binaries (universal) or copy the single one.
if [[ "${#BIN_PATHS[@]}" -gt 1 ]]; then
  echo "==> lipo -create -> universal binary"
  lipo -create -output "$MACOS_DIR/$BIN_NAME" "${BIN_PATHS[@]}"
else
  cp "${BIN_PATHS[0]}" "$MACOS_DIR/$BIN_NAME"
fi
chmod +x "$MACOS_DIR/$BIN_NAME"

# Generate .icns from src/images/icon.png.
if [[ -f "src/images/icon.png" ]]; then
  echo "==> generating AppIcon.icns"
  ICONSET="$(mktemp -d)/AppIcon.iconset"
  mkdir -p "$ICONSET"
  for size in 16 32 64 128 256 512; do
    dbl=$((size * 2))
    sips -z "$size"  "$size"  src/images/icon.png --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
    sips -z "$dbl"   "$dbl"   src/images/icon.png --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
  done
  iconutil -c icns -o "$RES_DIR/AppIcon.icns" "$ICONSET"
  rm -rf "$ICONSET"
fi

# Info.plist.
cat > "$CONTENTS_DIR/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>${APP_NAME}</string>
    <key>CFBundleDisplayName</key>
    <string>Rayview Meta</string>
    <key>CFBundleIdentifier</key>
    <string>${BUNDLE_ID}</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundleExecutable</key>
    <string>${BIN_NAME}</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
    <key>NSPrincipalClass</key>
    <string>NSApplication</string>
</dict>
</plist>
PLIST

# Ad-hoc sign so Gatekeeper does not immediately refuse to launch on the build host.
codesign --force --deep --sign - "$APP_DIR" >/dev/null 2>&1 || true

echo "==> Built: $APP_DIR"

# Optional .dmg packaging.
if command -v create-dmg >/dev/null 2>&1; then
  DMG_NAME="${APP_NAME}-${VERSION}-${DMG_ARCH}.dmg"
  DMG_PATH="${DIST_DIR}/${DMG_NAME}"
  rm -f "$DMG_PATH"
  echo "==> create-dmg -> $DMG_PATH"
  create-dmg \
    --volname "${APP_NAME} ${VERSION}" \
    --window-size 540 360 \
    --icon-size 96 \
    --icon "${APP_NAME}.app" 140 170 \
    --app-drop-link 400 170 \
    --hide-extension "${APP_NAME}.app" \
    "$DMG_PATH" "$APP_DIR" >/dev/null
  echo "==> Built: $DMG_PATH"
else
  echo "note: 'create-dmg' not installed; skipping .dmg packaging."
fi
