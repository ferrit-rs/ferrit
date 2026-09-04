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

Press `?` inside the app for the keybinding cheatsheet.

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
