#!/bin/sh
# The check of AI Factory. It runs in the repository after each code run of the agent.
# It does the same checks as .github/workflows/ci.yml on Linux.
# The UI end-to-end tests run only if the change touches ui/.
# Variables from the factory: FACTORY_BASE_SHA, FACTORY_BRANCH, FACTORY_ROUND.
set -eu

step() { echo; echo "== $*"; }

changed=""
if [ -n "${FACTORY_BASE_SHA:-}" ]; then
  changed=$(git diff --name-only "$FACTORY_BASE_SHA" HEAD || true)
fi

step "pnpm install"
pnpm install --frozen-lockfile --reporter=append-only

step "UI tests"
pnpm ui:test

# The app embeds ui/dist, so the UI builds before cargo.
step "UI build"
pnpm ui:build

if [ -z "$changed" ] || echo "$changed" | grep -q '^ui/'; then
  step "UI end-to-end tests (Chromium)"
  pnpm ui:e2e
else
  step "UI end-to-end tests: skipped, the change does not touch ui/"
fi

step "cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings

step "cargo test"
cargo test --workspace

step "All checks passed"
