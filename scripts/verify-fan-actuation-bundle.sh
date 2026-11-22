#!/bin/bash
set -euo pipefail

app=${1:?usage: verify-fan-actuation-bundle.sh /path/to/StatPulse.app}
repo_root=$(cd "$(dirname "$0")/.." && pwd)
resources="$app/Contents/Resources/fan-actuation"
plist="$resources/com.statpulse.fan-actuation.plist"
helper="$resources/fan-actuation-daemon"
installer="$resources/install-fan-actuation.sh"
authorizer="$resources/authorize-install.applescript"

fail() {
  echo "fan actuation bundle verification failed: $*" >&2
  exit 1
}

info="$app/Contents/Info.plist"
expected_id=$(plutil -extract identifier raw "$repo_root/src-tauri/tauri.conf.json")
expected_cli=$(plutil -extract name raw "$repo_root/package.json")
[ "$(plutil -extract CFBundleIdentifier raw "$info")" = "$expected_id" ] || fail "unexpected application bundle identifier"
[ "$(plutil -extract CFBundleExecutable raw "$info")" = "$expected_cli" ] || fail "unexpected application executable"
cli="$app/Contents/MacOS/$expected_cli"
[ -f "$cli" ] && [ ! -L "$cli" ] && [ -x "$cli" ] || fail "missing application executable: $expected_cli"

[ ! -e "$app/Contents/Library/LaunchDaemons/com.statpulse.fan-actuation.plist" ] || fail "SMAppService plist must not be bundled"
[ ! -e "$app/Contents/MacOS/fan-actuation-daemon" ] || fail "helper must not be a second app executable"
[ -x "$helper" ] || fail "missing executable helper resource"
[ -x "$installer" ] || fail "missing executable installer resource"
[ -r "$authorizer" ] || fail "missing AppleScript authorization resource"
[ -r "$plist" ] || fail "missing traditional launchd plist resource"
plutil -lint "$plist" >/dev/null

program=$(plutil -extract ProgramArguments.0 raw "$plist")
[ "$program" = "/Library/PrivilegedHelperTools/com.statpulse.fan-actuation" ] || fail "unexpected installed helper path"
if plutil -extract BundleProgram raw "$plist" >/dev/null 2>&1; then
  fail "traditional plist must not contain BundleProgram"
fi

bash -n "$installer"
# The shipped installer/authorizer must be byte-identical to their sources so
# a truncated or patched copy cannot sneak into the bundle.
cmp -s "$installer" "$repo_root/src-tauri/bundle/macos/fan-actuation/install-fan-actuation.sh"   || fail "shipped installer differs from its source"
cmp -s "$authorizer" "$repo_root/src-tauri/bundle/macos/fan-actuation/authorize-install.applescript"   || fail "shipped authorizer differs from its source"

codesign --verify --deep --strict --verbose=2 "$app"
echo "fan actuation installer bundle verified: $app"
