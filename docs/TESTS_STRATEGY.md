# Test strategy: what to steal from lazygit

> The harness this file feeds now exists (`ferrit::replay`, `test/scripts/`,
> `docs/PLAN_SELF_TESTING.md`); the backlog below is what is still worth
> turning into scripts.

Companion to [`PLAN_SELF_TESTING.md`](PLAN_SELF_TESTING.md) (the harness/mechanism
design). This file is the *behavior backlog* side: what lazygit's own test
suite covers, so `test/scripts/*.script` don't get invented from scratch per
phase.

## What lazygit has

- `pkg/integration/tests/**/*.go`, 551 files, one behavior per file:
  `Description`, `SetupRepo` (shell building a throwaway repo), `Run`
  (keypress sequence + screen assertions). Grouped by domain folder:
  `branch/`, `commit/`, `stash/`, `interactive_rebase/`, `cherry_pick/`,
  `bisect/`, `worktree/`, `conflicts/`, `diff/`, `patch_building/`,
  `reflog/`, `sync/`, `tag/`, `remote/`, `submodule/`, `undo/`,
  `filter_by_path/`, `filter_by_author/`, `filter_and_search/`,
  `custom_commands/`, `shell_commands/`, `config/`, `ui/`, `misc/`, `demo/`.
- `pkg/integration/components`: their driver DSL. Same idea as our
  `--replay` script format, different shape:

  ```go
  var Rename = NewIntegrationTest(NewIntegrationTestArgs{
      Description: "Rename a branch, replacing spaces in the name with dashes",
      SetupRepo: func(shell *Shell) { shell.EmptyCommit("commit") },
      Run: func(t *TestDriver, keys config.KeybindingConfig) {
          t.Views().Branches().Focus().
              Lines(Contains("master")).
              Press(keys.Branches.RenameBranch).
              Tap(func() {
                  t.ExpectPopup().Prompt().
                      Title(Contains("Enter new branch name")).
                      InitialText(Equals("master")).
                      Clear().Type("new branch name").Confirm()
              }).
              Lines(Contains("new-branch-name"))
      },
  })
  ```

  Maps directly onto a `.script`: `SetupRepo` -> `xtask fixture`, `Press` ->
  `key`, `Lines(Contains(...))` -> `expect-text`, popup prompt -> `type` +
  `key enter`.
- `docs/dev/Integration_Tests.md`: their methodology writeup (flake handling,
  headless run mode). `PLAN_SELF_TESTING.md` already independently solved
  the flakiness problem better (no pty, no sleeps), so nothing to import
  there beyond confirming we're not missing a gotcha.
- 126 plain `_test.go` files (`pkg/config`, `pkg/gocui`, `pkg/tasks`): ordinary
  Go unit tests, not behavior-relevant, not portable.

Not reusable: `pkg/gocui/*` (Go-specific TUI framework internals, no
ratatui equivalent needed), the Go code itself.

## Backlog mapping onto our phase table

Use lazygit's per-domain folder as the source of `Description` strings to
turn into `.script` files, phase by phase (phase numbers match the table in
`PLAN_SELF_TESTING.md`):

| Phase | lazygit folders to mine |
| --- | --- |
| 3 diff | `diff/`, `patch_building/` |
| 5 staging | `diff/` (staging-adjacent), `patch_building/` |
| 6 commit | `commit/`, `undo/` |
| 7 branches | `branch/` |
| 8 remote | `remote/`, `sync/` |
| 9 stash | `stash/` |
| 10 rebase | `interactive_rebase/`, `cherry_pick/`, `conflicts/` |
| later | `bisect/`, `worktree/`, `submodule/`, `tag/`, `reflog/`, `filter_by_path/`, `filter_by_author/`, `filter_and_search/`, `custom_commands/` |

Do not port `bisect/`, `submodule/`, `demo/` unless they get roadmapped;
skip domains ferrit doesn't plan to have.

## Working method

For each phase, before writing the `.script`:

1. Skim the matching lazygit folder, list every `Description` string.
2. Cut anything gocui/Go-specific (multi-pane popup chaining that doesn't
   map to our UI shape) or out of scope for the phase.
3. Translate the remaining descriptions 1:1 into `.script` files under
   `test/scripts/`, each with its `git ... -> "..."` golden block, per the
   format already defined in `PLAN_SELF_TESTING.md`.
4. TDD as usual: script red, implement, script green, same commit as the
   feature (per `AGENTS.md`).
