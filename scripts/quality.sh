#!/usr/bin/env bash

set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

usage() {
    cat <<'EOF'
Usage: scripts/quality.sh <format|fast|check|fix|scan>

  format run rustfmt check
  fast   rustfmt check and strict clippy
  check  every blocking local and CI quality gate
  fix    deterministic rustfmt and clippy fixes, then fast checks
  scan   full clippy warning dump and compact tracker
EOF
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || {
        printf 'error: missing command: %s\n' "$1" >&2
        exit 1
    }
}

check_explicit_paths() {
    printf '%s\n' '==> explicit Rust module paths'
    if rg -n --glob '*.rs' '\bsuper::|pub\(super\)' crates; then
        printf '%s\n' 'error: super paths are forbidden; use explicit crate:: paths and pub(crate)' >&2
        return 1
    fi
}

run_fast() {
    run_format

    printf '%s\n' '==> clippy'
    cargo clippy --locked --all-targets --all-features -- -D warnings
}

run_format() {
    check_explicit_paths

    printf '%s\n' '==> rustfmt'
    cargo fmt --all -- --check
}

run_tests() {
    require_command cargo-nextest

    ferrit_git_config=$(mktemp)
    trap 'rm -f "$ferrit_git_config"' EXIT

    printf '%s\n' '==> nextest'
    GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$ferrit_git_config" FERRIT_NO_DELTA=1 \
        cargo nextest run --locked --all-features
    rm -f "$ferrit_git_config"
    trap - EXIT
}

run_check() {
    run_fast

    printf '%s\n' '==> rustdoc'
    RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps --all-features --document-private-items

    require_command cargo-machete
    printf '%s\n' '==> cargo-machete'
    cargo machete

    require_command cargo-deny
    printf '%s\n' '==> cargo-deny'
    cargo deny check

    run_tests
}

run_fix() {
    printf '%s\n' '==> rustfmt --write'
    cargo fmt --all

    printf '%s\n' '==> clippy --fix'
    cargo clippy --locked --fix --all-targets --all-features \
        --allow-dirty --allow-staged -- -D warnings

    run_fast
}

run_scan() {
    require_command python3

    printf '%s\n' '==> bust clippy cache'
    while IFS= read -r source_file; do
        touch "$source_file"
    done < <(git ls-files '*.rs')

    printf '%s\n' '==> full clippy scan'
    cargo clippy --locked --all-targets --all-features -- \
        --force-warn clippy::all \
        --force-warn clippy::pedantic \
        --force-warn clippy::nursery \
        --force-warn clippy::cargo \
        > CLIPPY_FULL_DUMP.txt 2>&1

    python3 __SOP/strict-clippy-burndown.py
}

case "${1:-}" in
    format) run_format ;;
    fast) run_fast ;;
    check) run_check ;;
    fix) run_fix ;;
    scan) run_scan ;;
    -h | --help) usage ;;
    *) usage >&2; exit 2 ;;
esac
