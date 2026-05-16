#!/usr/bin/env bash
# Generates all required Tauri app icons from a single source PNG.
# Usage:  ./scripts/generate-icons.sh [path-to-1024x1024-source.png]
# Requires: npm install -g @tauri-apps/cli  OR  npx @tauri-apps/cli icon ...

set -e

SRC=${1:-"scripts/icon-source.png"}

if [ ! -f "$SRC" ]; then
  echo "Error: source image '$SRC' not found."
  echo "Provide a 1024x1024 PNG as the first argument, e.g.:"
  echo "  ./scripts/generate-icons.sh path/to/logo-1024.png"
  exit 1
fi

echo "Generating Tauri icons from: $SRC"
npx @tauri-apps/cli icon "$SRC"
echo "Icons generated in src-tauri/icons/"
