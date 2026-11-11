#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Lars Wallström

# Verify that the application version is identical in:
#   - frontend/package.json         ("version" field)
#   - frontend/package-lock.json    (top-level "version" field)
#   - backend/Cargo.toml            ([package] version)
#   - backend/Cargo.lock            ([[package]] dronefleetpro-backend version)
#
# Why: the project's versioning convention requires frontend and backend versions to be
# bumped in lockstep in the same PR, and Cargo.lock carries a copy of the
# backend version, just as package-lock.json carries a copy of the frontend
# version. A missed bump has previously required a separate chore PR
# to fix, so CI fails fast on any mismatch. POSIX sh + grep/sed only, so it
# can run anywhere without extra dependencies.

set -eu

cd "$(dirname "$0")/.."

pkg_version=$(sed -n 's/^[[:space:]]*"version":[[:space:]]*"\([^"]*\)".*/\1/p' frontend/package.json | head -n 1)

# The top-level "version" is the first one in the file; later matches belong
# to the packages[""] entry and to individual dependencies.
pkg_lock_version=$(sed -n 's/^[[:space:]]*"version":[[:space:]]*"\([^"]*\)".*/\1/p' frontend/package-lock.json | head -n 1)

# First `version = "..."` line inside the [package] section only.
cargo_version=$(sed -n '/^\[package\]/,/^\[/s/^version = "\([^"]*\)".*/\1/p' backend/Cargo.toml | head -n 1)

# In Cargo.lock the version line immediately follows the package name line.
lock_version=$(grep -A 1 '^name = "dronefleetpro-backend"$' backend/Cargo.lock | sed -n 's/^version = "\([^"]*\)".*/\1/p' | head -n 1)

status=0
[ -n "$pkg_version" ] || { echo "ERROR: could not read version from frontend/package.json" >&2; status=1; }
[ -n "$pkg_lock_version" ] || { echo "ERROR: could not read version from frontend/package-lock.json" >&2; status=1; }
[ -n "$cargo_version" ] || { echo "ERROR: could not read version from backend/Cargo.toml" >&2; status=1; }
[ -n "$lock_version" ] || { echo "ERROR: could not read dronefleetpro-backend version from backend/Cargo.lock" >&2; status=1; }
[ "$status" -eq 0 ] || exit "$status"

if [ "$pkg_version" != "$pkg_lock_version" ] || [ "$pkg_version" != "$cargo_version" ] || [ "$pkg_version" != "$lock_version" ]; then
    echo "Version lockstep FAILED:" >&2
    echo "  frontend/package.json      : $pkg_version" >&2
    echo "  frontend/package-lock.json : $pkg_lock_version" >&2
    echo "  backend/Cargo.toml         : $cargo_version" >&2
    echo "  backend/Cargo.lock         : $lock_version" >&2
    echo "All four must be bumped together in the same PR." >&2
    exit 1
fi

echo "Version lockstep OK: $pkg_version"
