#!/usr/bin/env bash
set -euo pipefail

PORT="${ONDA_TEST_PORT:-8787}"
TEST_ROOT="/tmp/onda-update-test"
INSTALLED_DIR="$TEST_ROOT/Installed"
MOCK_API_DIR="$TEST_ROOT/api"
FAKE_VERSION="${1:-99.0.0}"

echo "==> Building Onda.app (debug)..."
./scripts/bundle_app.sh debug

rm -rf "$TEST_ROOT"
mkdir -p "$INSTALLED_DIR" "$MOCK_API_DIR/repos/maximux13/onda" "$MOCK_API_DIR/download"

echo "==> Copying Onda.app to $INSTALLED_DIR/Onda.app..."
cp -R "target/debug/Onda.app" "$INSTALLED_DIR/Onda.app"

echo "==> Creating mock release archive Onda-macos-arm64.zip (v$FAKE_VERSION)..."
STAGED_APP="$TEST_ROOT/Staged/Onda.app"
mkdir -p "$TEST_ROOT/Staged"
cp -R "target/debug/Onda.app" "$STAGED_APP"
plutil -replace CFBundleShortVersionString -string "$FAKE_VERSION" "$STAGED_APP/Contents/Info.plist"
codesign --force --deep --sign - "$STAGED_APP"
ditto -c -k --sequesterRsrc --keepParent "$STAGED_APP" "$MOCK_API_DIR/download/Onda-macos-arm64.zip"

cat > "$MOCK_API_DIR/repos/maximux13/onda/releases" <<EOF
[
  {
    "tag_name": "v$FAKE_VERSION",
    "name": "Onda v$FAKE_VERSION (Local Test)",
    "created_at": "2026-11-18T00:00:00Z",
    "body": "Local mock release for testing Onda auto-updater.",
    "assets": [
      {
        "name": "Onda-macos-arm64.zip",
        "browser_download_url": "http://127.0.0.1:$PORT/download/Onda-macos-arm64.zip",
        "url": "http://127.0.0.1:$PORT/download/Onda-macos-arm64.zip"
      }
    ]
  }
]
EOF

echo "==> Starting local mock GitHub API server on http://127.0.0.1:$PORT..."
python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$MOCK_API_DIR" >/tmp/onda-mock-server.log 2>&1 &
SERVER_PID=$!
trap 'kill "$SERVER_PID" 2>/dev/null || true' EXIT

sleep 0.5
echo "==> Launching Onda.app with local updater env overrides..."
echo "    ONDA_UPDATE_API_URL=http://127.0.0.1:$PORT"
echo "    ONDA_UPDATE_CURRENT_VERSION=0.1.0"
echo "    ONDA_UPDATE_INSTALL_PATH=$INSTALLED_DIR/Onda.app"

NO_PROXY="127.0.0.1,localhost" \
no_proxy="127.0.0.1,localhost" \
ONDA_UPDATE_API_URL="http://127.0.0.1:$PORT" \
ONDA_UPDATE_CURRENT_VERSION="0.1.0" \
ONDA_UPDATE_INSTALL_PATH="$INSTALLED_DIR/Onda.app" \
"$INSTALLED_DIR/Onda.app/Contents/MacOS/onda"
