![Ferrit demo](demo-ferrit.gif)

# ferrit

*A git manager for the terminal.*

**Never open github.com to start a project again.**

Make a folder. Open ferrit in it. Press `i` to make it a repository, press `G`
to create it on GitHub and link it: first commit, `origin`, push. No browser,
no `git remote add`, no copying URLs.

```
 an empty folder                         your repository, on GitHub
 ┌─────────────┐   ferrit   ┌─────────────────────────────────────────┐
 │ my-project/ │ ─────────▶ │ i   git init                            │
 └─────────────┘            │ G   name · private/public · description │
                            │     first commit · origin · push        │
                            └─────────────────────────────────────────┘
```

Then it is the git manager you keep open, in a repository you just made or in one
you already have: stage and commit, branches and rebase, a dashboard of what has
happened in the repository, your git configuration, all in the terminal.

## In a repository you already have

Run `ferrit` in any repository. Everything below works there as well, with
nothing to set up first:

- **Understand it.** `D` slides in a dashboard over your panes: commits over
  time, who did what, the kind of work (feat, fix, docs...), the files that
  change most, the health of your branches.
- **Set git up.** `C` lists every git config key with the level it was set at
  and the file it came from, and edits it through `git config`; secrets stay
  hidden.
- **Work in it.** Stage files, hunks and single lines; commit (and choose the
  author of the commit with `Ctrl-A`, without touching git's config); branches,
  stash, conflicts, interactive rebase, fetch, pull and push, with your SSH
  passphrase asked in a popup instead of hanging.
- **Make it yours.** Click your name for the settings: a Terminal, Dark or
  Light theme, an accent colour, the mouse, the diff.

No remote yet? `G` (or `x`) offers to create it on GitHub and link it, the same
way as for a new folder.

## What you need

- **`gh`**, the GitHub CLI, signed in once (`gh auth login`), to create the
  repository. Ferrit never holds a token. Without `gh`, everything else works.
- **A working SSH key for GitHub**, to push. Ferrit uses your own `~/.ssh/config`
  alias, so the key you already use is the one that is used.
- **GitHub only**, for creating the remote. Other servers work as remotes you
  add yourself.
- **One repository at a time**: the one in the folder you open ferrit in.

## Ferrit and lazygit

The panes and the keys will feel familiar if you know lazygit, and ferrit covers
the same core workflow. What it adds is the rest of the life of a repository,
which lazygit leaves to other tools. Where lazygit does more today, the table
says so.

| Capability | ferrit | lazygit | Difference |
| --- | :---: | :---: | --- |
| Start in a folder that is not a repository | ✅ | ❌ | A welcome screen offers `git init` after a question that names the folder; `--path` keeps the error |
| Create the remote repository from the interface | ✅ | ❌ | `G` (or `x`) with no remote: GitHub through `gh`, private by default, first commit, then push |
| Repository statistics dashboard | ✅ | ❌ | `D`, a sheet over the dimmed panes: activity over time, contributors, kinds of change, hot files, branch health, with charts |
| Edit Git config from the interface | ✅ | ❌ | `C`: every key with its scope and origin, edited through `git config`, secrets hidden |
| Choose Git author from configured identities | ✅ | ❌ | In the commit popup, `Ctrl-A` cycles the identities git knows and git's own; for this run only, git config is not touched |
| Settings sheet | ✅ | ❌ | Click the author's name: a Terminal, Dark or Light theme (Dark and Light paint the whole screen), accent colour with a colour picker, mouse, wheel step, diff context, sign-off and command log, each saved to `config.toml` as you change it |
| Stage files, hunks and lines | ✅ | ✅ | Shared core workflow |
| Interactive rebase | ✅ | ✅ | Shared core workflow |
| Stash, branches, conflicts and remotes | ✅ | ✅ | Shared core workflow |
| Custom commands | ⚠️ | ✅ | Planned for Ferrit; lazygit supports user-defined commands |
| Worktree management | ⚠️ | ✅ | Planned for Ferrit; lazygit has built-in worktree actions |
| Gitflow integration | ⚠️ | ✅ | Planned for Ferrit; available in lazygit when Gitflow is installed |
| Git bisect workflow | ⚠️ | ✅ | Planned for Ferrit; built into lazygit's documented commit actions |

✅ available · ⚠️ planned. The comparison covers built-in features; lazygit can
be extended with custom commands.

## Install

```bash
cargo install ferrit
```

Requires Rust 1.86+ (edition 2024).

## Usage

```bash
ferrit          # open the TUI in the current folder
```

In a folder that is not a repository it opens the welcome screen. Press `?`
inside the app for the keybinding cheatsheet, `x` (or a right click) on a row for
the less common actions, `@` for the git commands ferrit ran.

## Configuration

`ferrit --config-path` prints where the settings file lives. Everything is
optional, the settings sheet writes it for you as you change things, and a wrong
value is reported at startup and only that part falls back to its default.

```toml
[theme]
scheme = "dark"          # "terminal" (your terminal's colours), "dark" or "light"
preset = "green"         # the accent: green, blue, purple, amber

[theme.colors]           # any of the 14 palette colours, "#rrggbb" or a name
add_line_bg = "#d6f5d6"

[ui]
mouse = true
wheel_step = 3
poll_secs = 10

[diff]
context = 3
ignore_whitespace = false

[commit]
sign_off = false

[log]
show_reads = false

[keys.global]            # remap any key but ctrl-c; also files, diff,
quit = "Q"               # branches, commits and stash
```

The help screen and the key hints follow the remapped keys.

## Building from source

```bash
git clone https://github.com/ferrit-rs/ferrit
cd ferrit
cargo run
```

## Contributing

Contributions welcome. All commits must be signed off (DCO):

```bash
git commit -s -m "your message"
```

By signing off you certify the [Developer Certificate of Origin](https://developercertificate.org/).

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for setup, checks, testing, and pull
request guidelines.

See [`MAINTAINERS.md`](MAINTAINERS.md) for the people maintaining this project.

## License

MIT. See [`LICENSE`](LICENSE).
