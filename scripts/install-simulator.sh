#!/usr/bin/env bash
set -euo pipefail
repository_root="$(cd "$(dirname "$0")/.." && pwd)"
device="${1:-booted}"
app_directory="$repository_root/target/simulator/Threds.app"
if [[ ! -d "$app_directory" ]]; then
    printf 'Build the app first with scripts/build-simulator.sh\n' >&2
    exit 1
fi
xcrun simctl terminate "$device" dev.threds.app >/dev/null 2>&1 || true
xcrun simctl install "$device" "$app_directory"
xcrun simctl launch "$device" dev.threds.app
