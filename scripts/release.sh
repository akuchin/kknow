#!/usr/bin/env bash
#
# Prepares a release: sets the version, checks the code, commits and tags.
#
# The version lives in one place: `[workspace.package]` in the root
# `Cargo.toml`. The CLI and the app inherit it, and Tauri takes it from there
# when `tauri.conf.json` has no `version`. The release workflow refuses a tag
# that differs from this version, so a forgotten step fails loudly instead of
# shipping `kknow_0.1.1_aarch64.dmg` under the tag v0.1.2.
#
# The script does not push: a push starts a public release, and that is a
# separate decision. It prints the command instead.
#
#   scripts/release.sh 0.1.2

set -euo pipefail

version="${1:-}"
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "usage: scripts/release.sh X.Y.Z" >&2
  exit 1
fi
tag="v$version"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

if [[ -n "$(git status --porcelain)" ]]; then
  echo "the working tree has changes; commit or stash them first" >&2
  exit 1
fi
if [[ "$(git branch --show-current)" != "main" ]]; then
  echo "release from main only" >&2
  exit 1
fi
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  echo "tag $tag already exists" >&2
  exit 1
fi

current="$(scripts/version.sh)"
if [[ "$current" == "$version" ]]; then
  echo "the version is already $version" >&2
  exit 1
fi

# Only the line in `[workspace.package]`: dependency lines also have
# `version = "…"`, but never at the start of a line.
sed -i.bak "s/^version = \"$current\"$/version = \"$version\"/" Cargo.toml
rm Cargo.toml.bak
if [[ "$(scripts/version.sh)" != "$version" ]]; then
  echo "could not set the version in Cargo.toml" >&2
  git checkout -- Cargo.toml
  exit 1
fi

# Updates the versions of the workspace crates in Cargo.lock, nothing else.
cargo update --workspace

cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

git commit -q -am "Release $version"
git tag -m "kknow $tag" "$tag"

echo
echo "ready: $tag ($current -> $version)"
echo "to publish: git push origin main $tag"
