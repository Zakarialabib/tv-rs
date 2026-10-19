#!/usr/bin/env bash
# Ordered crates.io release for the tv-rs workspace.
#
# Eight crates must be published in dependency order because every facade
# dependency carries `version` beside `path` (Cargo strips `path` at publish
# time, so the published facade resolves the crates.io versions):
#
#   tv-rs-transport -> tv-rs-discovery -> tv-rs-power -> tv-rs-roku
#     -> tv-rs-tizen -> tv-rs-webos -> tv-rs -> tv-rs-media
# `tv-rs-cli` is `publish = false` and is never released.
#
# Usage:
#   scripts/release.sh --dry-run                 # verify everything, publish nothing
#   scripts/release.sh --version 0.2.0 --dry-run # bump first, then verify
#   scripts/release.sh --version 0.2.0           # bump and publish
#   scripts/release.sh --token "$CRATES_IO_TOKEN" --version 0.2.0
#
# Flags:
#   --version X.Y.Z   Bump [workspace.package] version and every
#                     [workspace.dependencies] path crate version to X.Y.Z.
#   --dry-run         Run `cargo publish --dry-run` for all crates (no upload).
#   --token TOKEN     crates.io API token (defaults to $CRATES_IO_TOKEN).
#   --skip-tests      Skip `cargo test --workspace` in the preflight.
#   --yes             Do not prompt before the first real publish.
#
# Exit codes: 0 success, 1 preflight failure, 2 bad usage.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

# Publish order: every path dependency must reach crates.io before its dependents.
# `transport` is the leaf every other workspace crate builds on — its
# `Unsupported` vocabulary is unconditional, while its TLS connector and frame
# reader sit behind the default-off `ws` feature that only `tizen`/`webos`
# enable. `discovery`/`power`/`roku` take the vocabulary alone. The facade
# depends on all five protocol crates; `media` depends on nothing in the
# workspace and is published last.
ORDER=(tv-rs-transport tv-rs-discovery tv-rs-power tv-rs-roku tv-rs-tizen tv-rs-webos tv-rs tv-rs-media)
# Crates that are not facade dependencies — exempt from the version-pin check
# because they are never referenced from [workspace.dependencies].
UNPINNED=(tv-rs tv-rs-media)
VERSION=""
DRY_RUN=0
TOKEN="${CRATES_IO_TOKEN:-}"
SKIP_TESTS=0
ASSUME_YES=0

usage() {
    sed -n '2,30p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --version)
            [[ $# -ge 2 ]] || { echo "error: --version needs a value" >&2; exit 2; }
            VERSION="$2"; shift 2 ;;
        --dry-run) DRY_RUN=1; shift ;;
        --token)
            [[ $# -ge 2 ]] || { echo "error: --token needs a value" >&2; exit 2; }
            TOKEN="$2"; shift 2 ;;
        --skip-tests) SKIP_TESTS=1; shift ;;
        --yes|-y) ASSUME_YES=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) echo "error: unknown argument '$1'" >&2; usage >&2; exit 2 ;;
    esac
done

step() { printf '\n==> %s\n' "$*"; }
fail() { printf 'error: %s\n' "$*" >&2; exit 1; }

# --- version bump -----------------------------------------------------------
if [[ -n "$VERSION" ]]; then
    [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-].*)?$ ]] \
        || fail "--version must look like 1.2.3 (got '$VERSION')"

    step "Bumping workspace version to $VERSION"
    # [workspace.package] version
    python - "$VERSION" <<'PY'
import re, sys, pathlib
version = sys.argv[1]
path = pathlib.Path("Cargo.toml")
text = path.read_text(encoding="utf-8")

# Only rewrite the workspace package version and the path-crate versions.
text, n_pkg = re.subn(
    r'(?m)^(version\s*=\s*")[^"]+(")',
    rf'\g<1>{version}\g<2>',
    text,
    count=1,
)
text, n_dep = re.subn(
    r'(tv-rs-[a-z]+\s*=\s*\{[^}]*?version\s*=\s*")[^"]+(")',
    rf'\g<1>{version}\g<2>',
    text,
)
path.write_text(text, encoding="utf-8")
print(f"workspace.package version updated ({n_pkg}), dependency versions updated ({n_dep})")
PY

    # Keep Cargo.lock in sync before any publish attempt.
    cargo update --workspace --offline >/dev/null 2>&1 || cargo update --workspace >/dev/null
    step "Version bump complete"
fi

# --- preflight --------------------------------------------------------------
step "Preflight"
[[ -f LICENSE ]] || fail "LICENSE is missing (cargo publish would reject an MIT crate)"
[[ -f Cargo.toml ]] || fail "Cargo.toml is missing"
command -v cargo >/dev/null || fail "cargo is not on PATH"

# Every protocol crate must be declared in [workspace.dependencies] with a
# local `version` beside `path`; the facade (`tv-rs`) is the root package and
# is published from its own [package] table.
for pkg in "${ORDER[@]}"; do
    if printf '%s\n' "${UNPINNED[@]}" | grep -qx "$pkg"; then
        continue
    fi
    grep -qE "^${pkg} *= *\{.*version *=" Cargo.toml \
        || fail "workspace dependency '$pkg' with a version pin is missing from Cargo.toml"
done
grep -qE '^name *= *"tv-rs"' Cargo.toml \
    || fail "root package 'tv-rs' is missing from Cargo.toml"

step "cargo check --workspace --all-targets"
cargo check --workspace --all-targets

if [[ "$SKIP_TESTS" -eq 0 ]]; then
    step "cargo test --workspace"
    cargo test --workspace
fi

# --- publish ----------------------------------------------------------------
PUBLISH_ARGS=()
if [[ "$DRY_RUN" -eq 1 ]]; then
    # A dry run publishes nothing, so an uncommitted working tree is fine and
    # expected while iterating on the release metadata.
    PUBLISH_ARGS+=(--dry-run --allow-dirty)
else
    if [[ -n "$(git status --porcelain)" ]]; then
        fail "working tree is dirty — commit before publishing (a dirty publish cannot be reproduced)"
    fi
    if [[ -z "$TOKEN" ]]; then
        fail "no crates.io token: pass --token or set CRATES_IO_TOKEN"
    fi
    PUBLISH_ARGS+=(--token "$TOKEN")
    if [[ "$ASSUME_YES" -eq 0 ]]; then
        read -r -p "Publish ${#ORDER[@]} crates to crates.io in order? [y/N] " reply
        [[ "$reply" =~ ^[Yy]$ ]] || fail "aborted by user"
    fi
fi

for pkg in "${ORDER[@]}"; do
    step "cargo publish -p $pkg ${PUBLISH_ARGS[*]}"
    if output=$(cargo publish -p "$pkg" "${PUBLISH_ARGS[@]}" 2>&1); then
        printf '%s\n' "$output"
        if [[ "$DRY_RUN" -eq 0 ]]; then
            printf 'published %s; sleeping 20s so the crates.io index catches up\n' "$pkg"
            sleep 20
        fi
    else
        printf '%s\n' "$output" >&2
        if [[ "$DRY_RUN" -eq 1 ]] \
            && grep -q "no matching package named \`tv-rs-" <<<"$output"; then
            # First publish: workspace path deps are not on crates.io yet, so any
            # crate depending on them cannot resolve during a dry run. Expected
            # before the first real release, not a metadata defect.
            printf '\nwarning: dry-run of %s skipped — a workspace dependency is not on\n' "$pkg"
            printf 'crates.io yet. Re-run the dry run after the earlier crates in the\n'
            printf 'order have been published once.\n'
        else
            fail "cargo publish failed for $pkg"
        fi
    fi
done

step "Done: ${ORDER[*]}"
if [[ "$DRY_RUN" -eq 1 ]]; then
    printf 'dry run only — nothing was uploaded\n'
fi
