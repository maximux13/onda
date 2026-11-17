#!/usr/bin/env bash
set -euo pipefail

PROFILE="${1:-debug}"
if [ "$PROFILE" = "release" ]; then
    cargo build --release
    BIN_PATH="target/release/onda"
    APP_DIR="target/release/Onda.app"
else
    cargo build
    BIN_PATH="target/debug/onda"
    APP_DIR="target/debug/Onda.app"
fi

rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"
mkdir -p "$APP_DIR/Contents/Resources"

cp "$BIN_PATH" "$APP_DIR/Contents/MacOS/onda"
cp "Info.plist" "$APP_DIR/Contents/Info.plist"
codesign --force --deep --sign - "$APP_DIR"

echo "Built $APP_DIR"
