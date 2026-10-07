export const projectConfig = {
  name: "Ferrit",
  tagline: "The everyday Git manager for your terminal.",
  repositoryUrl: "https://github.com/ferrit-rs/ferrit",
  pillars: [
    {
      number: "01",
      title: "Git fidelity",
      body: "Ferrit keeps Git behavior recognizable. Reads use libgit2 where it gives stable access, while the real Git process handles configuration, hooks, diff drivers, and command behavior that users already rely on.",
    },
    {
      number: "02",
      title: "Visible boundaries",
      body: "The headless Git domain stays separate from the Ratatui surface. Process creation, output logging, secret redaction, and askpass behavior live behind explicit seams that can be tested and explained.",
    },
    {
      number: "03",
      title: "A usable release",
      body: "Ferrit is more than a terminal screen. Tests, replay scripts, documentation, release tags, an MIT license, and a public repository leave a trail another engineer can inspect and continue.",
    },
  ],
  steps: [
    {
      label: "Brief",
      title: "Make everyday Git work calmer.",
      detail:
        "Ferrit starts from a familiar problem: Git is powerful, but a long command sequence can make state hard to see. The project turns that workflow into a focused terminal surface without pretending Git rules do not matter.",
    },
    {
      label: "Architecture",
      title: "Keep the core independent from the TUI.",
      detail:
        "Repository behavior lives in a headless domain layer. Ratatui renders the current state, while Git integration stays behind a narrow process seam with explicit ownership and testable boundaries.",
    },
    {
      label: "Implementation",
      title: "Build the actions users actually need.",
      detail:
        "Status, files, branches, commits, reflog, stash, diffs, staging at file, hunk, and line level, remote operations, conflict handling, settings, themes, and keymaps form one coherent workflow.",
    },
    {
      label: "Validation",
      title: "Test behavior as a workflow.",
      detail:
        "Integration tests, replay scripts, flow specifications, strict lints, and redacted command logs validate more than isolated functions. They exercise the terminal behavior users will actually experience.",
    },
    {
      label: "Shipped",
      title: "Leave a public, inspectable result.",
      detail:
        "Ferrit reaches a public v0.10.0 release with documentation, release history, an MIT license, and a repository that makes the implementation and its trade-offs available to the next engineer.",
    },
  ],
  layers: [
    [
      "Core",
      "Git state, domain types, repository operations, and the rules that make actions predictable. This layer stays independent from terminal rendering.",
    ],
    [
      "Boundary",
      "libgit2 reads, real Git commands, GitHub CLI, SSH and HTTPS prompts, configuration, and secret-safe process logging. Each external edge has one visible entry point.",
    ],
    [
      "Delivery",
      "Ratatui views, keyboard flows, replay fixtures, integration coverage, documentation, release tags, and the path from a checkout to a usable terminal tool.",
    ],
  ],
  details: [
    { label: "Project type", value: "Rust terminal application" },
    { label: "Author", value: "Richard Lavoura" },
    { label: "Stack", value: "Rust · Ratatui · Git" },
    { label: "License", value: "MIT · public" },
  ],
  stats: [
    { value: "29k+", label: "Rust lines in src" },
    { value: "411", label: "Commits on main" },
    { value: "8", label: "Release tags" },
    { value: "59", label: "Integration tests" },
  ],
} as const;
