#!/usr/bin/env bash
# Run the unit test suite. Uses bats from PATH, or hints how to get it via nix.
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if command -v bats >/dev/null 2>&1; then
    exec bats "$DIR"
fi

echo "bats-core not found on PATH." >&2
echo "Run via nix:  nix shell nixpkgs#bats-core --command bats $DIR" >&2
exit 127
