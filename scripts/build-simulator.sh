#!/usr/bin/env bash
set -euo pipefail
repository_root="$(cd "$(dirname "$0")/.." && pwd)"
target_directory="$repository_root/target"
app_directory="$target_directory/simulator/Threds.app"
cd "$repository_root"
mkdir -p "$target_directory"
build_output="$(mktemp "$target_directory/threds-build.XXXXXX")"
trap 'rm -f "$build_output"' EXIT
IPHONEOS_DEPLOYMENT_TARGET=15.0 CARGO_TARGET_DIR="$target_directory" \
    cargo +1.98.1 build --locked --target aarch64-apple-ios-sim --bin threds \
    --message-format=json-render-diagnostics "$@" > "$build_output"
executable="$(python3 - "$build_output" <<'PY'
import json
import sys
with open(sys.argv[1]) as output:
    executables = [event["executable"] for event in map(json.loads, output)
        if event.get("reason") == "compiler-artifact"
        and event.get("target", {}).get("name") == "threds"
        and "bin" in event.get("target", {}).get("kind", [])
        and event.get("executable")]
if len(executables) != 1:
    sys.exit("Expected one Threds executable in Cargo build output")
print(executables[0])
PY
)"
mkdir -p "$app_directory"
cp "$executable" "$app_directory/threds"
cp "$repository_root/scripts/Info.plist" "$app_directory/Info.plist"
codesign --force --sign - "$app_directory"
printf 'Built simulator app: %s\n' "$app_directory"
