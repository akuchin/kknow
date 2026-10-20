#!/usr/bin/env bash
#
# Installs the built app into /Applications.
#
# `open -a kknow ~/notes` and "Open With" in Finder work only with an app that
# LaunchServices knows, that is, an installed app, not the one in
# `target/release/bundle`. Spotlight launch also needs this.
#
# Run: `pnpm install:app` (builds first), or run this script directly if the
# build is fresh. The first argument sets a different directory. Tests use it
# to keep the real /Applications untouched.
#
#   scripts/install-app.sh              # into /Applications
#   scripts/install-app.sh /tmp/test    # into any other directory

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
app="$root/target/release/bundle/macos/kknow.app"
apps="${1:-/Applications}"
dest="$apps/kknow.app"

if [[ ! -d "$app" ]]; then
  echo "built app not found: $app" >&2
  echo "build it first: pnpm build:app" >&2
  exit 1
fi

mkdir -p "$apps"

# Do not replace a running app: its files would disappear under the process.
# Quit only the app at the install location. A dev build runs from another
# directory, so leave it alone.
if pgrep -f "^$dest/Contents/MacOS/" >/dev/null 2>&1; then
  echo "quitting the running app..."
  # `is running` stops AppleScript from launching the app only to quit it.
  # `quit` is a normal exit, so a pending edit gets saved and auto-sync can
  # finish.
  osascript -e 'if application id "dev.kknow.desktop" is running then tell application id "dev.kknow.desktop" to quit' || true

  for _ in $(seq 1 20); do
    pgrep -f "^$dest/Contents/MacOS/" >/dev/null 2>&1 || break
    sleep 0.5
  done

  if pgrep -f "^$dest/Contents/MacOS/" >/dev/null 2>&1; then
    echo "the app did not quit in 10 seconds; close it and try again" >&2
    exit 1
  fi
fi

# Remove the old version fully instead of copying over it. Otherwise files
# that the new build no longer has would stay.
rm -rf "$dest"
# `ditto`, not `cp`: it keeps permissions and extended attributes. For a
# bundle these are the signature and the flags the system needs to launch it.
ditto "$app" "$dest"

# Register the app with LaunchServices. Without this, `open -a kknow` and
# Finder can remember the old location for some time.
lsregister="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
if [[ -x "$lsregister" ]]; then
  "$lsregister" -f "$dest" || true
fi

echo "installed: $dest"
