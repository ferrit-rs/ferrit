# LazyGit architecture reference and Ferrit plan

Status: living architecture document. Reference checked from `../ferrit-references/tui/lazygit` and `../ferrit-references/tui/lazygitrs`.

## Target shape

```text
input/event
    |
    v
route by mode/context
    |
    v
controller/action  ------->  domain state mutation
    |                               |
    v                               v
service/use-case  ----------->  Git port
                                    |
                                    v
                            subprocess or git2 adapter

state/snapshot  --->  projection/presentation  --->  widgets/terminal
      ^                                             |
      +------------- refresh/result/event ----------+
```

Core rule: domain and Git code do not know terminal layout. Input routing does not execute Git commands. Presentation reads state and produces rows or widgets. Controllers coordinate intent, side effects and refresh.

## LazyGit complete architecture

LazyGit is a large Go application. Its package boundaries are the useful reference, not a file-by-file port target.

```text
lazygit/
├── cmd/                         CLI entrypoints and command wiring
├── main.go                      process entrypoint
├── pkg/
│   ├── app/
│   │   ├── app.go               application lifecycle and dependency setup
│   │   ├── entry_point.go       CLI to application startup
│   │   ├── errors.go             process-level errors
│   │   ├── daemon/               background daemon and rebase lifecycle
│   │   └── types/                app-level shared types
│   ├── commands/
│   │   ├── git.go                Git command facade
│   │   ├── git_cmd_obj_builder.go
│   │   ├── git_cmd_obj_runner.go command construction and execution
│   │   ├── git_commands/         one Git capability or use-case per file
│   │   │   ├── branch*            branch reads and mutations
│   │   │   ├── commit*            commit reads and mutations
│   │   │   ├── diff.go            diff commands
│   │   │   ├── file*             file and working-tree loading
│   │   │   ├── remote*           remote reads and mutations
│   │   │   ├── stash*            stash reads and mutations
│   │   │   ├── status.go          status and working-tree state
│   │   │   ├── sync.go            fetch, pull, push
│   │   │   ├── tag*              tag reads and mutations
│   │   │   ├── worktree*         worktree reads and mutations
│   │   │   └── rebase.go          rebase operations
│   │   ├── git_config/            cached Git config access
│   │   ├── hosting_service/       GitHub and hosting integration
│   │   ├── models/                command-facing Git models
│   │   ├── oscommands/            process, PTY and platform adapters
│   │   ├── patch/                 patch parse, transform and build
│   │   └── direnv/                environment integration
│   ├── config/
│   │   ├── app_config.go          normalized application config
│   │   ├── user_config.go         user file loading
│   │   ├── user_config_validation.go
│   │   ├── keybinding.go           keybinding config
│   │   ├── theme_config.go         theme config
│   │   ├── side_panel.go           layout config
│   │   └── platform files          OS-specific defaults
│   ├── constants/                 stable links and constants
│   ├── env/                       environment discovery
│   ├── fakes/                     test fakes
│   ├── gui/
│   │   ├── gui.go                 GUI composition root
│   │   ├── gui_driver.go           event loop and terminal driver
│   │   ├── input and global handlers
│   │   ├── layout and main_panels  layout composition
│   │   ├── views.go                view registration and rendering
│   │   ├── context/                stateful view contexts and list models
│   │   │   ├── main_context.go
│   │   │   ├── working_tree_context.go
│   │   │   ├── local_commits_context.go
│   │   │   ├── branches_context.go
│   │   │   ├── remotes_context.go
│   │   │   ├── stash_context.go
│   │   │   ├── tags_context.go
│   │   │   ├── worktrees_context.go
│   │   │   ├── commit_files_context.go
│   │   │   ├── prompt/menu/confirmation contexts
│   │   │   ├── filtered_list and list view models
│   │   │   └── traits/               reusable context contracts
│   │   ├── controllers/           user intent and side-effect orchestration
│   │   │   ├── global_controller.go
│   │   │   ├── main_view_controller.go
│   │   │   ├── files_controller.go
│   │   │   ├── branches_controller.go
│   │   │   ├── commits_files_controller.go
│   │   │   ├── staging_controller.go
│   │   │   ├── stash_controller.go
│   │   │   ├── remotes_controller.go
│   │   │   ├── tags_controller.go
│   │   │   ├── worktrees_controller.go
│   │   │   ├── rebase, diff, patch, filter and search controllers
│   │   │   └── helpers/             focused cross-controller operations
│   │   ├── filetree/                tree model, nodes, filtering and cursor
│   │   ├── modes/                   diff, filter, cherry-pick and base-commit modes
│   │   ├── popup/                   popup lifecycle
│   │   ├── presentation/            state-to-renderable transformation
│   │   │   ├── branches, commits, files, status, remotes, tags, stashes
│   │   │   ├── graph/                graph cells and graph layout
│   │   │   ├── icons/                icon selection
│   │   │   ├── authors/              author presentation
│   │   │   └── loader.go             loading presentation
│   │   ├── services/                reusable GUI-facing services
│   │   │   └── custom_commands/      custom command resolution and menus
│   │   ├── status/                   status manager
│   │   ├── style/                    color and text styling
│   │   ├── types/                    GUI contracts, modes and refresh types
│   │   └── mergeconflicts/           conflict discovery and rendering
│   ├── gocui/                       terminal library boundary
│   ├── i18n/                        translations
│   ├── integration/                 end-to-end test clients and scenarios
│   ├── jsonschema/                  config schema
│   ├── logs/                        logging and tailing
│   ├── tasks/                       asynchronous task coordination
│   ├── theme/                       theme definitions
│   ├── updates/                     update checks
│   ├── utils/                       narrow generic utilities
│   └── snake/                       embedded helper feature
└── scripts/                         build and release tooling
```

Important LazyGit patterns:

1. `commands` is capability-oriented. It does not become one giant Git file.
2. `gui/context` owns screen-local state and list behavior.
3. `gui/controllers` owns intent and mutations. Controllers are split by feature.
4. `gui/presentation` turns models into display data. Rendering stays thin.
5. `gui/filetree` is a real domain component, not a helper hidden in a pane.
6. `gui/modes` isolates temporary workflows from the normal screen.
7. `commands/models` keeps Git models close to the Git boundary.
8. `commands/oscommands` isolates process and platform behavior.
9. Tests follow the same feature boundaries as production code.
10. The scale is justified by feature count. Ferrit should copy boundaries, not LazyGit's quantity of files.

## LazyGitRS patterns worth keeping

`lazygitrs` confirms the Rust translation:

```text
src/
├── app.rs
├── config/                     config and app state
├── git/                        capability modules and Git models
├── gui/
│   ├── context/                screen state
│   ├── controller/             feature controllers
│   ├── modes/                  temporary workflows
│   ├── presentation/           render projections
│   ├── input.rs                event routing
│   ├── layout.rs               layout only
│   ├── popup.rs                popup lifecycle
│   ├── scroll.rs               scroll policy
│   └── views.rs                view composition
├── model/                      domain models and file tree
├── os/                         command, platform and TTY boundary
├── pager/                      diff rendering and highlighting
└── themes/                     theme data
```

Rust adaptation rules:

- `mod.rs` is a composition root, not a dumping ground.
- Use private modules by default. Expose only true crate boundaries.
- Use `crate::...` explicit paths. No `pub use`, `super::` or `pub(super)`.
- Keep `pub(crate)` only where a sibling feature genuinely consumes an API.
- Use enums for closed state machines and Strum for stable names, parsing and iteration.
- Keep payload enums explicit when variants carry data. Do not force Strum onto a shape it cannot model cleanly.
- Prefer typed commands, intents and results over stringly-typed controller plumbing.
- Keep adapters at edges. Domain models should not depend on `ratatui`, terminal types or subprocess details.
- Make async or background work return events/results. Do not let workers mutate arbitrary UI state.
- Put projection code between state and draw code. Avoid drawing directly from Git structs.

## Ferrit current architecture

```text
src/
├── config/                       settings and app configuration
├── git/
│   ├── model.rs, port.rs         domain-facing Git types and boundary
│   ├── repo/                     subprocess-backed repository adapter
│   │   ├── status.rs             status reads
│   │   ├── log.rs                commit history and decorations
│   │   ├── branches.rs           branch reads
│   │   ├── blob.rs               blob reads
│   │   ├── diff.rs               diff reads
│   │   ├── index.rs              index mutations
│   │   ├── commit.rs             commit mutations and commit metadata
│   │   ├── rebase.rs             rebase and stopped-operation mutations
│   │   ├── remotes.rs            remote operations
│   │   ├── stashes.rs            stash operations
│   │   ├── gitconfig.rs          Git config operations
│   │   ├── statistics/           statistics queries
│   │   └── read.rs               shared worktree and command output helpers
│   └── fake.rs                   test Git implementation
├── replay/                       deterministic scenario harness
├── theme/                        palette and theme configuration
└── tui/
    ├── components/               feature UI components
    │   ├── files/                 tree, projection, navigation and keys
    │   ├── dashboard/              state/loading, view/rendering, charts, tables, text
    │   ├── settings/               sheet orchestration, theme editor and row projection
    │   ├── create_remote/           state/form/input and popup projection/rendering
    │   ├── commit_editor/           commit flow, draft/input state and popup rendering
    │   ├── keybar/                  bar layout, generated help lines and screen rendering
    │   ├── panes/                 generic pane orchestration
    │   ├── diff/                  diff feature
    │   ├── dashboard/             dashboard feature
    │   ├── git_config/             Git config feature
    │   └── ...                    branches, commits, remotes, stash, settings
    ├── controllers/actions.rs     resolved action mutation and navigation
    ├── keymap/                   actions, bindings, contexts and defaults
    ├── event.rs, reducer.rs       intent and state mutation boundaries
    ├── input.rs                   key/mouse/popup routing
    ├── scene.rs, view.rs          read-only view context
    ├── row_lines/                 render projections
    ├── widgets/                   reusable widgets
    ├── runtime.rs                 event loop
    └── terminal.rs                terminal lifecycle
```

Current result is already structurally close to the useful LazyGit shape:

```text
Git capability modules  ->  GitPort  ->  App snapshot  ->  TUI components
                                              |
input -> keymap -> action controller -> reducer/state
                         |
                         +---------- refresh/event
```

## Plan to converge further

### Phase 0: guardrails, done or active

- Keep `scripts/quality.sh fast` cheap for commit: format, explicit-path checks, fast clippy/checks.
- Keep full tests, deny, docs and strict clippy on push.
- Keep `pub_use = "deny"` and explicit path checks.
- Keep zero `allow(dead_code)`.
- Keep every visible change in `CHANGELOG.md`.
- Keep `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, tests and docs deterministic.

### Phase 1: finish the Rust domain boundary

1. Audit `src/git/model.rs` and `src/git/port.rs` by capability. Move models that only belong to one capability beside that capability, while keeping shared domain models centralized.
2. Separate read models from mutation commands where names currently mix both.
3. Introduce typed result/event names only where current strings or tuples obscure ownership.
4. Keep `Repo` as a thin adapter facade. New behavior goes into capability modules first, forwarding method second.
5. Compare every `FakeGit` method with the port contract. Add missing contract scenarios before adding convenience methods.

### Phase 2: make TUI boundaries match LazyGit

1. Split `tui/components` by feature where a file owns multiple independent workflows.
2. Move screen-local state out of `App` only when a component has a stable state boundary. Do not create empty context wrappers.
3. Grow `tui/controllers/` from `actions.rs` into feature modules as action groups become independent: files, commits, branches, remotes, stash, diff and settings.
4. Keep `input.rs` as router only. It may identify context and produce intent, but must not contain feature mutation logic.
5. Keep `scene.rs` and `view.rs` read-only. Push formatting and row conversion into feature projections.
6. Extract a `tui/modes/` namespace when diff, rebase or another workflow gets temporary state and exit rules. Do not create it before a real mode exists.
7. Keep `widgets/` terminal-generic. Feature-specific layout belongs under its component.

### Phase 3: projection and refresh model

1. Define explicit feature projections for branches, commits, files, remotes, stash and status.
2. Make projections own display-only data: labels, icons, styles, row heights and hit targets.
3. Make controllers request refresh through typed events/results, not direct redraw assumptions.
4. Make loading and error states part of feature state, not ad hoc draw branches.
5. Keep snapshot creation cheap and deterministic. Heavy Git work stays in the adapter or worker boundary.

### Phase 4: configuration and keymap

1. Keep `keymap/action.rs`, `binding.rs`, `context.rs` and `defaults.rs` separate.
2. Use Strum for closed string enums and config-facing iteration. Keep explicit matches for payload actions such as `Focus(Pane)`.
3. Separate config file parsing, normalized settings and runtime preferences.
4. Validate key collisions and reserved actions at config load time.
5. Keep Git config UI under `components/git_config`, separate from app config.

### Phase 5: platform and Git edge isolation

1. Consolidate process, askpass, SSH and terminal-specific code under explicit adapter boundaries.
2. Keep `git2` and subprocess details out of TUI modules.
3. Keep image decoding and terminal protocol detection under `tui/image` and terminal modules.
4. Add platform modules only when behavior differs. Do not abstract identical code prematurely.

### Phase 6: test architecture

1. Mirror production feature folders in integration tests once a test file grows past the AGENTS threshold.
2. Use `FakeGit` for app behavior and real temporary repositories for Git behavior.
3. Add property tests for every free-form Git parser.
4. Add controller tests around intent to state transition, not only final rendering.
5. Add projection tests for row order, selection identity, styles and hit areas.
6. Keep end-to-end tests for a small set of complete user flows.

## Loop rule

After each architecture batch:

```text
inspect ownership -> split only real boundaries -> migrate imports
        -> add/update tests -> run fast gate -> run full gate
        -> inspect file sizes and dependencies -> update this plan/changelog
        -> commit and push -> repeat while a concrete boundary remains
```

Stop when remaining differences are feature scope, not architecture defects. Do not copy LazyGit's accidental complexity or Go-specific patterns. Keep the same separation of concerns with Rust ownership, typed enums, explicit modules, Strum where useful, and narrow visibility.
