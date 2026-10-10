#!/usr/bin/env bash

set -euo pipefail

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"
git config core.hooksPath .githooks
printf 'Git hooks enabled: %s\n' "$(git config --get core.hooksPath)"
