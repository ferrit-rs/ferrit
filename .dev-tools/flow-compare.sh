#!/usr/bin/env bash
# Dev-only helper, not part of ferrit itself: run one user flow (a file under
# test/flows/) against lazygit and against ferrit, shoot both at every step
# with real Terminal.app screenshots (tui-shot.sh), record the git state each
# step left in each repository, and bundle everything into one report that
# shows lazygit | ferrit side by side with the state diff.
#
# lazygit is the reference: a step whose git state differs is a workflow that
# does not do the same thing in ferrit. The screenshots are for a human (or an
# agent) to compare the look; nothing here fails a build.
#
# Usage: .dev-tools/flow-compare.sh [--repo PATH] test/flows/<name>.flow
#
# The repository is a throwaway COPY, never the original: a `fixture NAME` from
# the flow, or, with `--repo PATH` (or a `repo PATH` line, `.` is this repo),
# a copy of a real repository with its history, branches, stash and uncommitted
# changes. In a copy the remotes are pointed at nothing (no fetch or push can
# reach the real one) and your global git config is kept, so it looks and
# commits as your own repository does.
#
# Flow file, one directive per line, `#` starts a comment, shell quoting:
#   fixture NAME        a `ferrit --fixture` repository, one per program
#   repo PATH           a copy of this real repository, one per program
#   step NAME KEYS...   send KEYS (see tui-shot.sh: Enter, Space, C-x, text:<s>,
#                       anything else is typed literally), then shoot
#   sh "COMMAND"        run COMMAND in the repository copy. Before the first step
#                       it is setup (reset to a known state); after, it is an
#                       edit made behind the programs' back (an editor)
#   focus "TEXT"       what this flow checks; the report shows it and the analysis stays inside it
#   not "TEXT"         what it leaves to another flow (name that flow)
#   note "TEXT"         what the next step does and should show, for the report
#
# Needs Screen Recording and Automation permissions (see __SOP/visual-verify.md),
# tmux, lazygit and a debug build of ferrit.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo_src=""
if [ "$1" = --repo ]; then
  repo_src="$2"
  shift 2
fi
flow="$1"
name="$(basename "$flow" .flow)"
out="$root/.verify-shots/flows/$name"
ferrit="$root/target/debug/ferrit"

fixture="$(awk '$1 == "fixture" { print $2 }' "$flow")"
[ -n "$repo_src" ] || repo_src="$(awk '$1 == "repo" { print $2 }' "$flow")"
[ -n "$fixture$repo_src" ] || { echo "$flow: no fixture or repo line" >&2; exit 1; }
[ -z "$repo_src" ] || repo_src="$(cd "$root" && cd "$repo_src" && pwd)"

cargo build --quiet --manifest-path "$root/Cargo.toml"
# The run about to be replaced is kept as before/ (screenshots only): the report
# shows it next to the new one for every fix. Only a run of the same flow, and only
# when the previous one had screenshots; before/ of before/ is not kept.
keep="$(mktemp -d)"
if [ -d "$out/ferrit" ]; then
  mkdir -p "$keep/ferrit" "$keep/lazygit"
  cp "$out"/ferrit/*.png "$keep/ferrit/" 2>/dev/null || true
  cp "$out"/lazygit/*.png "$keep/lazygit/" 2>/dev/null || true
fi
rm -rf "$out"
mkdir -p "$out"
if [ -d "$keep/ferrit" ]; then
  mkdir -p "$out/before"
  cp -R "$keep/ferrit" "$keep/lazygit" "$out/before/"
fi
rm -rf "$keep"

# What a step left behind, comparable across programs: no commit ids or dates,
# which differ per run.
state() {
  git -C "$1" --no-optional-locks status --porcelain=v1 -uall
  echo "-- log"
  git -C "$1" --no-optional-locks log -n 30 --format=%s
  echo "-- branches"
  git -C "$1" --no-optional-locks branch --format='%(refname:short)'
  echo "-- stash"
  git -C "$1" --no-optional-locks stash list --format=%gs
  echo "-- index"
  git -C "$1" --no-optional-locks diff --cached --stat
}

for target in lazygit ferrit; do
  dir="$out/$target"
  home="$dir/home"
  mkdir -p "$home"
  if [ -n "$repo_src" ]; then
    repo="$dir/$(basename "$repo_src")"
    rsync -a --exclude=/target --exclude=/.verify-shots "$repo_src/" "$repo/"
    for remote in $(git -C "$repo" remote); do
      git -C "$repo" remote set-url "$remote" "file:///nonexistent/$remote"
      git -C "$repo" remote set-url --push "$remote" "file:///nonexistent/$remote"
    done
    [ ! -f "$HOME/.gitconfig" ] || cp "$HOME/.gitconfig" "$home/.gitconfig"
    [ ! -d "$HOME/.config/git" ] || { mkdir -p "$home/.config"; cp -R "$HOME/.config/git" "$home/.config/git"; }
  else
    repo="$("$ferrit" --fixture "$fixture" --into "$dir/fx")"
  fi
  session="flow-$target"
  tmux kill-session -t "$session" 2>/dev/null || true

  # An empty HOME keeps the user's own configuration (and state) out of both
  # programs. lazygit would otherwise open a welcome popup and check for updates.
  case "$target" in
    lazygit)
      mkdir -p "$home/Library/Application Support/lazygit"
      printf 'disableStartupPopups: true\nupdate:\n  method: never\ngit:\n  autoFetch: false\n  autoRefresh: true\nrefresher:\n  refreshInterval: 1\n' \
        > "$home/Library/Application Support/lazygit/config.yml"
      cmd="cd '$repo' && HOME='$home' lazygit"
      ;;
    ferrit)
      cmd="cd '$repo' && HOME='$home' FERRIT_NO_GRAPHICS=1 '$ferrit'"
      ;;
  esac

  note=""
  started=0
  while IFS= read -r line; do
    case "$line" in '' | '#'*) continue ;; esac
    eval "set -- $line"
    directive="$1"
    shift
    case "$directive" in
      # Run in the copy, like an editor or another terminal would. Before the
      # first step it is the setup; after, it happens mid-flow, and each program
      # has to notice on its own.
      # lazygit only rereads the disk every refreshInterval (1 s here, 10 s by
      # default), so a mid-flow edit gets 2 s to be noticed before the next key.
      sh)
        (cd "$repo" && HOME="$home" sh -c "$1")
        [ "$started" = 0 ] || sleep 2
        continue
        ;;
      note) note="$1"; continue ;;
      step) ;;
      *) continue ;;
    esac
    step="$1"
    shift
    started=1
    [ -z "${note:-}" ] || printf '%s\n' "$note" > "$dir/$step.note.txt"
    note=""
    FERRIT_SHOT_SESSION="$session" FERRIT_SHOT_CMD="$cmd" \
      "$root/.dev-tools/tui-shot.sh" "flows/$name/$target/$step" "$@" >/dev/null
    state "$repo" > "$dir/$step.git.txt"
    echo "$target: $step"
  done < "$flow"

  tmux kill-session -t "$session" 2>/dev/null || true
  winfile="$root/.verify-shots/.terminal_window_id.$session"
  if [ -f "$winfile" ]; then
    osascript -e "tell application \"Terminal\" to close window id $(cat "$winfile")" >/dev/null 2>&1 || true
    rm -f "$winfile"
  fi
done

"$root/.dev-tools/flow-report.sh" --no-open "$name"
echo "Screenshots and git states are in place; the visual analyses are not written yet."
echo "Ask for /compare-lazygit $name to write them, or run: .dev-tools/flow-report.sh $name"
