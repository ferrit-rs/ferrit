#!/bin/sh
# Publish the version in Cargo.toml: checks, the crate on crates.io, then the
# release tag (named exactly like that version, 0.6.0, no "v"), with the tag's
# message taken from that version's CHANGELOG.md section.
#
#   scripts/release.sh              dry run: every check, nothing changes
#   scripts/release.sh --execute    do it, after asking you to type the version
#   scripts/release.sh --execute --yes        ... without asking
#   scripts/release.sh --skip-tests           skip `cargo test` (the slow check)
#
# Order, and why: checks, then push `main`, then `cargo publish`, then the tag.
# Publishing is the one step that cannot be undone, so it comes last among the
# steps that can fail for a reason we can check first, and the tag is made only
# once the crate is really out: a tag never names a version that was not
# published. If the publish fails, nothing was tagged, so running the script
# again is clean. If the tag push fails after a successful publish, the script
# says so and the tag command to run by hand.
#
# Needs a crates.io login (`cargo login`) or CARGO_REGISTRY_TOKEN, and push
# access to `origin`. Run it from a clean `main` after the release commit
# (`chore: release ferrit X.Y.Z`: version bumped, CHANGELOG section dated).

set -eu

REMOTE=origin
BRANCH=main

execute=0
assume_yes=0
skip_tests=0
for arg in "$@"; do
    case "$arg" in
        --execute) execute=1 ;;
        --yes) assume_yes=1 ;;
        --skip-tests) skip_tests=1 ;;
        -h | --help)
            sed -n '2,/^$/p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "unknown option: $arg (see --help)" >&2
            exit 2
            ;;
    esac
done

cd "$(dirname "$0")/.."

step() { printf '\n==> %s\n' "$*"; }
fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}
# What a step would do; in a dry run it is only printed.
act() {
    if [ "$execute" -eq 1 ]; then
        printf '+ %s\n' "$*"
        "$@"
    else
        printf '(dry run) would run: %s\n' "$*"
    fi
}

# --- the version and its changelog section -------------------------------

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
[ -n "$version" ] || fail "no version found in Cargo.toml"
# The tag is the version in Cargo.toml, exactly: 0.6.0, no "v" prefix.
tag="$version"
printf 'Releasing ferrit %s, tag %s (%s)\n' "$version" "$tag" \
    "$([ "$execute" -eq 1 ] && echo EXECUTE || echo 'dry run, nothing will change')"

step "CHANGELOG.md has a section for $version"
heading=$(grep -n "^## \[$version\] - [0-9]\{4\}-[0-9]\{2\}-[0-9]\{2\}\$" CHANGELOG.md | head -n 1 || true)
[ -n "$heading" ] || fail "CHANGELOG.md has no '## [$version] - YYYY-MM-DD' heading"

# The section body: from its heading to the next '## [' heading or the block of
# link references at the end, whichever comes first.
notes=$(awk -v h="## [$version] " '
    index($0, h) == 1 { inside = 1; next }
    inside && (/^## \[/ || /^\[[^]]+\]: /) { exit }
    inside { print }
' CHANGELOG.md | sed -e :a -e '/^\n*$/{$d;N;ba' -e '}' | sed '/./,$!d')
[ -n "$notes" ] || fail "the $version section of CHANGELOG.md is empty"

# Anything left under [Unreleased] is a change nobody wrote a release note for.
pending=$(awk '
    /^## \[Unreleased\]/ { inside = 1; next }
    inside && (/^## \[/ || /^\[[^]]+\]: /) { exit }
    inside && NF { print }
' CHANGELOG.md)
[ -z "$pending" ] || fail "[Unreleased] still has entries: move them under $version or release later"

grep -q "^\[$version\]: " CHANGELOG.md || fail "CHANGELOG.md has no [$version]: link reference"
grep -q "^\[Unreleased\]: .*compare/$tag\.\.\.HEAD" CHANGELOG.md \
    || fail "the [Unreleased] link must compare $tag...HEAD"
printf 'found: %s\n' "${heading#*:}"
printf '%s lines of release notes\n' "$(printf '%s\n' "$notes" | wc -l | tr -d ' ')"

# --- the repository ------------------------------------------------------

step "the working tree is a clean $BRANCH"
[ "$(git rev-parse --abbrev-ref HEAD)" = "$BRANCH" ] || fail "not on $BRANCH"
[ -z "$(git status --porcelain)" ] || fail "the working tree is not clean"

git rev-parse -q --verify "refs/tags/$tag" >/dev/null && fail "tag $tag already exists here"

if git fetch --quiet --tags "$REMOTE" 2>/dev/null; then
    git rev-parse -q --verify "refs/remotes/$REMOTE/$BRANCH" >/dev/null \
        || fail "$REMOTE has no $BRANCH"
    git merge-base --is-ancestor "$REMOTE/$BRANCH" HEAD \
        || fail "$REMOTE/$BRANCH has commits this $BRANCH does not (pull first)"
    ahead=$(git rev-list --count "$REMOTE/$BRANCH..HEAD")
    printf '%s commit(s) will be pushed with the release\n' "$ahead"
else
    [ "$execute" -eq 0 ] || fail "cannot reach $REMOTE"
    echo "warning: cannot reach $REMOTE, so its state was not checked (a real run would stop here)"
fi

# --- the checks ----------------------------------------------------------

step "cargo fmt, clippy, doc"
cargo fmt --check
cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps

if [ "$skip_tests" -eq 1 ]; then
    step "cargo test (skipped by --skip-tests)"
else
    step "cargo test"
    cargo test
fi

step "cargo publish --dry-run (packages and builds the crate, uploads nothing)"
cargo publish --dry-run

# --- the release ---------------------------------------------------------

if [ "$execute" -eq 1 ] && [ "$assume_yes" -eq 0 ]; then
    printf '\nThis publishes ferrit %s to crates.io. That cannot be undone.\n' "$version"
    printf 'Type the version (%s) to continue: ' "$version"
    read -r answer
    [ "$answer" = "$version" ] || fail "not confirmed, nothing was published"
fi

step "push $BRANCH to $REMOTE"
act git push "$REMOTE" "$BRANCH"

step "publish ferrit $version to crates.io"
act cargo publish

step "tag $tag with the CHANGELOG notes, and push it"
notes_file=$(mktemp)
trap 'rm -f "$notes_file"' EXIT
printf 'ferrit %s\n\n%s\n' "$version" "$notes" >"$notes_file"
if ! act git tag -a "$tag" -F "$notes_file" || ! act git push "$REMOTE" "$tag"; then
    fail "ferrit $version IS published; only tagging failed. Run: git tag -a $tag -F <notes> && git push $REMOTE $tag"
fi

if [ "$execute" -eq 1 ]; then
    printf '\nDone: ferrit %s is on crates.io and tagged %s.\n' "$version" "$tag"
else
    printf '\nDry run complete: every check passed. Run again with --execute to publish.\n'
    printf 'The tag message would be:\n\n'
    sed 's/^/    /' "$notes_file"
fi
