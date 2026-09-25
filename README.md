# ferrit

A lazygit-style terminal UI for git, written in Rust.

`ferrit` brings a fast, keyboard-driven TUI to everyday git work: staging hunks,
crafting commits, browsing branches and the reflog, resolving conflicts, and
running interactive rebases without leaving the terminal.

> Status: early development. Not usable yet.

## Why

- **Fast**: native Rust, no runtime, instant startup.
- **Keyboard first**: every action reachable without the mouse.
- **Readable diffs**: syntax-aware, hunk-level staging.
- **Safe**: destructive actions always ask first.

## Install

```bash
cargo install ferrit
```

Requires Rust 1.85+ (edition 2024).

## Usage

```bash
ferrit          # open the TUI in the current repo
```

Press `?` inside the app for the keybinding cheatsheet, `x` (or a right
click) on a row for the less common actions, `@` for the git commands ferrit
ran.

## Configuration

`ferrit --config-path` prints where the settings file lives. Everything is
optional; a wrong value is reported at startup and only that part falls back
to its default.

```toml
[theme]
base = "dark"            # or "light"
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

See [`MAINTAINERS.md`](MAINTAINERS.md) for the people maintaining this project.

## License

MIT. See [`LICENSE`](LICENSE).
