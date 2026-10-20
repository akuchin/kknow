#!/usr/bin/env bash
#
# Prints the kknow version: `[workspace.package]` in the root `Cargo.toml`.
# The release script and the release workflow both read it here, so they
# cannot disagree about where the version is.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
version="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/Cargo.toml")"

if [[ -z "$version" || "$version" == *$'\n'* ]]; then
  echo "expected one top-level version line in Cargo.toml" >&2
  exit 1
fi
echo "$version"
