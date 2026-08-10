#!/bin/zsh
set -euo pipefail

SCRIPT_DIR="${0:A:h}"
PROJECT_DIR="${SCRIPT_DIR:h}"
APP_NAME="FileManager"
APP_BUNDLE="${PROJECT_DIR}/dist/${APP_NAME}.app"

cd "${PROJECT_DIR}"

echo "Building release binary..."
cargo build --release

echo "Creating ${APP_BUNDLE}..."
mkdir -p "${APP_BUNDLE}/Contents/MacOS" "${APP_BUNDLE}/Contents/Resources"
cp "${PROJECT_DIR}/target/release/filemanager" "${APP_BUNDLE}/Contents/MacOS/filemanager"
cp "${PROJECT_DIR}/packaging/Info.plist" "${APP_BUNDLE}/Contents/Info.plist"
chmod 755 "${APP_BUNDLE}/Contents/MacOS/filemanager"

# Ad-hoc signing makes the locally built bundle behave consistently when moved.
/usr/bin/codesign --force --deep --sign - "${APP_BUNDLE}"

if [[ "${1:-}" == "--install" ]]; then
    echo "Installing ${APP_NAME}.app into /Applications..."
    /usr/bin/ditto "${APP_BUNDLE}" "/Applications/${APP_NAME}.app"
    echo "Installed: /Applications/${APP_NAME}.app"
else
    echo "Created: ${APP_BUNDLE}"
    echo "Run 'scripts/release.sh --install' to copy it into /Applications."
fi

