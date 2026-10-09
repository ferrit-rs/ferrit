# Ferrit documentation

Start with [`architecture.md`](architecture.md), then the decisions in [`adr/`](adr/).
The `PLAN_*` files are the design history: one file per phase, written before the
code, with the milestones and the definition of done. [`PLAN_0_GENERAL.md`](PLAN_0_GENERAL.md)
is the index.

## Decisions

| ADR | Decision |
| --- | --- |
| [1](adr/0001-git2-for-reads-subprocess-for-writes.md) | Read with `git2`, write with the `git` subprocess |
| [2](adr/0002-replay-scripts-as-integration-tests.md) | A replay harness for end-to-end behaviour |
| [3](adr/0003-paint-the-whole-screen.md) | Ferrit paints its whole screen |
| [4](adr/0004-strict-lints-as-policy.md) | A strict lint policy, enforced in CI |
| [5](adr/0005-a-port-in-front-of-git.md) | A port in front of git |

## Plans by theme

| Theme | Plans |
| --- | --- |
| Foundations | [0 overview](PLAN_0_GENERAL.md), [1 layout](PLAN_1_LAYOUT.md), [2 git backend](PLAN_2_GIT_BACKEND.md), [3 diff view](PLAN_3_DIFF_VIEW.md) |
| Interaction | [4 scrolling](PLAN_4_SCROLL_BEHAVIOR.md), [5 clicks](PLAN_5_CLICK_BEHAVIOR.md), [12 polish](PLAN_12_POLISH.md) |
| Git workflow | [6 staging](PLAN_6_STAGING.md), [7 commit](PLAN_7_COMMIT.md), [8 branches](PLAN_8_BRANCHES.md), [9 remote](PLAN_9_REMOTE.md), [10 stash](PLAN_10_STASH.md), [11 rebase](PLAN_11_REBASE.md) |
| Beyond lazygit | [13 dashboard](PLAN_13_DASHBOARD.md), [14 git config](PLAN_14_GIT_CONFIG.md), [15 create remote](PLAN_15_CREATE_REMOTE.md), [16 start without a repo](PLAN_16_START_WITHOUT_REPO.md), [19 dashboard sheet](PLAN_19_DASHBOARD_SHEET.md) |
| Look and settings | [17 settings](PLAN_17_SETTINGS.md), [18 themes](PLAN_18_THEMES.md) |
| Code health | [20 typed errors](PLAN_20_TYPED_ERRORS.md), [21 git port](PLAN_21_GIT_PORT.md), [22 app split](PLAN_22_APP_SPLIT.md), [23 test support](PLAN_23_TEST_SUPPORT.md), [24 drawing reads the app](PLAN_24_DRAW_VIEW.md) |

## Testing

[`PLAN_SELF_TESTING.md`](PLAN_SELF_TESTING.md) describes the harness,
[`TESTS_STRATEGY.md`](TESTS_STRATEGY.md) the behaviours still worth scripting.
[`INSPIRATION.md`](INSPIRATION.md) lists the projects ferrit learned from and
[`BUG_IMAGE_PREVIEW.md`](BUG_IMAGE_PREVIEW.md) is a bug write-up.
