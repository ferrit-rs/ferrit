#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration test: a failed setup or a bad slice is the assertion"
)]
//! The replay harness itself (`docs/PLAN_SELF_TESTING.md` ST0 to ST2): the
//! script parser, the runner, the fixtures and the hidden command line. The
//! scripts under `test/scripts/` are run by `tests/replay.rs`; these tests are
//! about the machinery, so a broken harness cannot pass every script by
//! silently checking nothing.

use std::path::Path;
use std::process::Command;

use ferrit::replay::fixture::{self, Fixture};
use ferrit::replay::runner::{self, Options};
use ferrit::replay::script::{self, Directive, Expect};
use ferrit::replay::tape;

fn run(text: &str) -> Result<runner::Outcome, runner::Failure> {
    // A script that does not parse is reported as a failure at its line, so a
    // typo in a test script fails that test with a message, not a panic.
    let script = script::parse(text).map_err(|e| runner::Failure {
        line: e.line,
        message: e.message,
        frame: String::new(),
    })?;
    runner::run(&script, &Options::default())
}

fn failure(text: &str) -> runner::Failure {
    run(text).expect_err("the script should fail")
}

// ------------------------------------------------------------------ parser

#[test]
fn every_directive_parses() {
    let parsed = script::parse(
        "fixture canonical\nsize 120x40\nresize 40x20\nkey 2 j ctrl-d\nasync-key f\ntype \"hi\\n\"\n\
         refresh\nexec commit -m x\nwrite f \"a\\nb\"\nconfig \"[ui]\\nmouse = false\"\nsnapshot after-1\n\
         expect-text \"x\"\nexpect-no-text \"y\"\ngit log -1 -> \"x\"\ngit status => \"\"\n",
    )
    .unwrap();
    let directives: Vec<&Directive> = parsed.steps.iter().map(|s| &s.directive).collect();
    assert_eq!(directives.len(), 15);
    assert!(matches!(directives[0], Directive::Fixture(n) if n == "canonical"));
    assert!(matches!(directives[1], Directive::Size(120, 40)));
    assert!(matches!(directives[2], Directive::Resize(40, 20)));
    assert!(matches!(directives[3], Directive::Key(k) if k.len() == 3));
    assert!(matches!(directives[5], Directive::Type(t) if t == "hi\n"));
    assert!(matches!(directives[8], Directive::Write { content, .. } if content == "a\nb"));
    assert!(matches!(
        directives[13],
        Directive::Git { args, expect: Expect::Contains(e) } if args == &["log", "-1"] && e == "x"
    ));
    assert!(
        matches!(directives[14], Directive::Git { expect: Expect::Exact(e), .. } if e.is_empty())
    );
}

#[test]
fn comments_blank_lines_and_line_numbers() {
    let parsed = script::parse("# heading\n\nkey j   # trailing\n   \nkey k\n").unwrap();
    let lines: Vec<usize> = parsed.steps.iter().map(|s| s.line).collect();
    assert_eq!(lines, [3, 5]);
}

#[test]
fn a_hash_inside_quotes_is_text_not_a_comment() {
    let parsed = script::parse("expect-text \"a # b\"\nexpect-text 'x # y'\n").unwrap();
    assert!(matches!(&parsed.steps[0].directive, Directive::ExpectText(t) if t == "a # b"));
    assert!(matches!(&parsed.steps[1].directive, Directive::ExpectText(t) if t == "x # y"));
}

#[test]
fn single_quotes_are_literal_and_double_quotes_escape() {
    let parsed = script::parse("type 'a\\nb'\ntype \"a\\\"b\\\\c\\td\"\n").unwrap();
    assert!(matches!(&parsed.steps[0].directive, Directive::Type(t) if t == "a\\nb"));
    assert!(matches!(&parsed.steps[1].directive, Directive::Type(t) if t == "a\"b\\c\td"));
}

#[test]
fn a_quoted_arrow_is_an_argument_not_the_expectation() {
    let parsed = script::parse("git log --format=\"->\" -> \"x\"\n").unwrap();
    assert!(matches!(
        &parsed.steps[0].directive,
        Directive::Git { args, .. } if args == &["log", "--format=->"]
    ));
}

#[test]
fn each_mistake_is_reported_with_its_line() {
    let cases = [
        ("key j\nfrobnicate\n", 2, "unknown directive"),
        ("key\n", 1, "at least one key"),
        ("key not-a-key\n", 1, "not a key"),
        ("size 12\n", 1, "not WxH"),
        ("size 0x10\n", 1, "not WxH"),
        ("type \"open\n", 1, "unterminated"),
        ("type \"a\\qb\"\n", 1, "unknown escape"),
        ("\n\ngit log\n", 3, "needs"),
        ("git -> \"x\"\n", 1, "arguments before the arrow"),
        ("git log -> \"a\" \"b\"\n", 1, "exactly one string"),
        ("snapshot bad label\n", 1, "exactly one argument"),
        ("snapshot bad/label\n", 1, "not a label"),
        ("refresh now\n", 1, "no argument"),
        ("write onlypath\n", 1, "path and a string"),
        ("expect-text a b\n", 1, "exactly one"),
    ];
    for (text, line, needle) in cases {
        let error = script::parse(text).expect_err(text);
        assert_eq!(error.line, line, "{text:?}: {error}");
        assert!(error.message.contains(needle), "{text:?}: {error}");
    }
}

// ------------------------------------------------------------------ runner

#[test]
fn a_passing_script_returns_its_snapshots_in_order() {
    let outcome = run("fixture canonical\nsnapshot first\nkey 2\nsnapshot second\n").unwrap();
    let labels: Vec<(usize, &str)> = outcome
        .frames
        .iter()
        .map(|f| (f.index, f.label.as_str()))
        .collect();
    assert_eq!(labels, [(1, "first"), (2, "second")]);
    assert_ne!(
        outcome.frames[0].text, outcome.frames[1].text,
        "the key changed the screen"
    );
    assert!(outcome.frames[1].text.contains("main.rs"));
}

#[test]
fn a_failed_expectation_names_the_line_and_carries_the_frame() {
    let f = failure("fixture canonical\n\nkey 2\nexpect-text \"no such text\"\n");
    assert_eq!(f.line, 4);
    assert!(f.message.contains("no such text"), "{f}");
    assert!(
        f.frame.contains("Files"),
        "the frame the run saw is attached:\n{}",
        f.frame
    );
}

#[test]
fn expect_no_text_fails_when_the_text_is_there() {
    let f = failure("fixture canonical\nexpect-no-text \"canonical\"\n");
    assert_eq!(f.line, 2);
    assert!(f.message.contains("not to contain"), "{f}");
}

#[test]
fn a_script_without_a_fixture_or_with_one_out_of_place_fails_before_running() {
    assert_eq!(failure("key j\n").line, 0);
    let f = failure("key j\nfixture canonical\n");
    assert!(f.message.contains("must be the first"), "{f}");
    let f = failure("fixture nope\n");
    assert!(f.message.contains("unknown fixture"), "{f}");
}

#[test]
fn git_checks_compare_by_substring_or_exactly() {
    let ok = "fixture history\ngit log -1 --format=%s -> \"thre\"\ngit log -1 --format=%s => \"three\"\n";
    assert!(run(ok).is_ok());
    let f = failure("fixture history\ngit log -1 --format=%s => \"thre\"\n");
    assert!(f.message.contains("expected exactly"), "{f}");
    let f = failure("fixture history\ngit log -1 --format=%s -> \"zzz\"\n");
    assert!(f.message.contains("expected it to contain"), "{f}");
}

#[test]
fn exec_and_write_change_the_fixture_and_refresh_shows_it() {
    let outcome = run(
        "fixture history\nkey 2\nexpect-text \"working tree clean\"\nwrite f \"edited\\n\"\nrefresh\n\
         expect-text \"M f\"\nexec add f\nrefresh\nexpect-text \"M  f\"\ngit status --porcelain -> \"M  f\"\n\
         snapshot done\n",
    )
    .unwrap();
    assert_eq!(outcome.frames.len(), 1);
}

#[test]
fn a_failing_exec_stops_the_run() {
    let f = failure("fixture history\nexec no-such-git-subcommand\n");
    assert_eq!(f.line, 2);
    assert!(f.message.contains("failed"), "{f}");
}

#[test]
fn size_and_resize_change_the_frame() {
    let outcome =
        run("fixture canonical\nsize 80x24\nsnapshot small\nresize 100x30\nsnapshot big\n")
            .unwrap();
    let dims = |text: &str| {
        let rows: Vec<&str> = text.lines().collect();
        (
            rows.iter().map(|r| r.chars().count()).max().unwrap(),
            rows.len(),
        )
    };
    assert_eq!(dims(&outcome.frames[0].text), (80, 24));
    assert_eq!(dims(&outcome.frames[1].text), (100, 30));
}

#[test]
fn type_sends_characters_and_a_newline_is_enter() {
    // `s` on Files opens the stash popup; typing then Enter stashes.
    let outcome = run(
        "fixture canonical\nkey 2 s\ntype \"parked\\n\"\ngit stash list -> \"parked\"\nsnapshot after\n",
    )
    .unwrap();
    assert!(outcome.frames[0].text.contains("working tree clean"));
}

#[test]
fn config_reopens_the_app_with_the_given_settings() {
    let f = run(
        "fixture canonical\nconfig \"[keys.global]\\nhelp = \\\"H\\\"\\n\"\nkey H\nexpect-text \"toggle this help\"\n",
    );
    assert!(f.is_ok(), "{:?}", f.err());
    let f = failure(
        "fixture canonical\nconfig \"[keys.global]\\nhelp = \\\"H\\\"\\n\"\nkey ?\nexpect-text \"toggle this help\"\n",
    );
    assert_eq!(f.line, 4, "the old key is unbound: {f}");
}

#[test]
fn async_key_waits_for_background_work_without_sleeping() {
    // Another clone pushes; the fetch runs on a thread, and the script sees
    // the result only once the app is idle.
    let script = "fixture remote\n\
                  write {other}/g \"new\\n\"\n\
                  exec -C {other} add g\n\
                  exec -C {other} commit -q -m elsewhere\n\
                  exec -C {other} push -q origin main\n\
                  async-key f\n\
                  git log origin/main --format=%s -1 -> \"elsewhere\"\n\
                  expect-text \"↓1\"\n";
    assert!(run(script).is_ok(), "{:?}", run(script).err());
}

#[test]
fn an_extreme_terminal_size_does_not_abort_the_harness() {
    // Whether the app draws or panics at 1x1, the run must come back with an
    // `Ok` or a `Failure`, never unwind through the caller.
    let outcome = std::panic::catch_unwind(|| run("fixture canonical\nsize 1x1\nkey 2\n"));
    assert!(outcome.is_ok(), "the harness itself must not panic");
}

// ---------------------------------------------------------------- fixtures

fn head(dir: &Path) -> String {
    fixture::git_output(dir, &["rev-parse", "HEAD"]).unwrap().0
}

#[test]
fn a_fixture_builds_the_same_commits_every_time() {
    for name in fixture::NAMES {
        let a = Fixture::build(name, None).unwrap();
        let b = Fixture::build(name, None).unwrap();
        assert_eq!(head(&a.dir), head(&b.dir), "{name}");
        assert_ne!(a.dir, b.dir, "each build is its own directory");
    }
}

#[test]
fn canonical_is_the_screen_of_the_layout_plan() {
    let f = Fixture::build("canonical", None).unwrap();
    let out = |args: &[&str]| fixture::git_output(&f.dir, args).unwrap().0;
    assert_eq!(out(&["log", "--format=%s"]).lines().count(), 4);
    assert!(
        out(&["branch"]).contains("feat/tui-skeleton")
            && out(&["branch"]).contains("fix/parse-args")
    );
    let status = out(&["status", "--porcelain"]);
    assert!(status.contains(" M src/main.rs"), "{status}");
    assert!(status.contains("?? docs/notes.md"), "{status}");
    assert!(status.contains("A  Cargo.lock"), "{status}");
    assert_eq!(out(&["stash", "list"]), "");
    assert_eq!(
        f.dir.file_name().unwrap(),
        "canonical",
        "the repo name is the fixture name"
    );
}

#[test]
fn the_other_fixtures_are_what_their_names_say() {
    let out = |f: &Fixture, args: &[&str]| fixture::git_output(&f.dir, args).unwrap();
    let conflict = Fixture::build("conflict", None).unwrap();
    assert!(
        out(&conflict, &["status", "--porcelain"])
            .0
            .contains("UU f")
    );
    let detached = Fixture::build("detached", None).unwrap();
    assert!(
        !out(&detached, &["symbolic-ref", "-q", "HEAD"]).1,
        "HEAD is detached"
    );
    let remote = Fixture::build("remote", None).unwrap();
    assert!(remote.origin.as_ref().unwrap().exists() && remote.other.as_ref().unwrap().exists());
    assert_eq!(
        out(&remote, &["rev-parse", "--abbrev-ref", "@{u}"]).0,
        "origin/main"
    );
    assert_eq!(out(&remote, &["status", "-sb"]).0, "## main...origin/main");
}

#[test]
fn a_fixture_does_not_read_the_users_git_config() {
    let f = Fixture::build("history", None).unwrap();
    let out = |key: &str| {
        fixture::git_output(&f.dir, &["config", "--local", key])
            .unwrap()
            .0
    };
    assert_eq!(out("commit.gpgsign"), "false");
    assert_eq!(out("user.email"), "fixture@ferrit.invalid");
    assert_eq!(out("pull.rebase"), "false");
}

#[test]
fn a_temporary_fixture_is_removed_and_an_into_one_is_kept() {
    let root;
    {
        let f = Fixture::build("history", None).unwrap();
        root = f.root.clone();
        assert!(root.exists());
    }
    assert!(!root.exists(), "removed on drop");

    let into = std::env::temp_dir().join(format!("ferrit-harness-keep-{}", std::process::id()));
    {
        let f = Fixture::build("history", Some(&into)).unwrap();
        assert_eq!(f.root, into);
    }
    assert!(into.join("history").exists(), "kept");
    std::fs::remove_dir_all(&into).unwrap();
}

#[test]
fn an_unknown_fixture_lists_the_known_ones() {
    let error = Fixture::build("nope", None).unwrap_err();
    assert!(
        error.contains("canonical") && error.contains("remote"),
        "{error}"
    );
}

#[test]
fn placeholders_expand_to_the_fixtures_paths() {
    let f = Fixture::build("remote", None).unwrap();
    let text = f.expand("{dir}|{origin}|{other}");
    assert_eq!(text.split('|').count(), 3);
    assert!(!text.contains('{'));
}

// -------------------------------------------------------------------- tapes

#[test]
fn a_script_becomes_a_tape_of_keys_typing_and_screenshots() {
    let parsed = script::parse(
        "fixture canonical\nkey 2 space ctrl-d enter\ntype \"hi\"\nsnapshot done\nexpect-text \"x\"\n",
    )
    .unwrap();
    let tape = tape::tape("demo", &parsed).unwrap();
    assert!(tape.starts_with("Output test/shots/demo.gif"), "{tape}");
    assert!(tape.contains("ferrit --fixture canonical"), "{tape}");
    for line in [
        "Type \"2\"",
        "Space",
        "Ctrl+d",
        "Enter",
        "Type \"hi\"",
        "Screenshot test/shots/demo-done.png",
    ] {
        assert!(tape.lines().any(|l| l == line), "{line:?} missing:\n{tape}");
    }
    assert!(
        !tape.contains("expect"),
        "assertions are not part of a tape"
    );
}

#[test]
fn a_script_that_changes_things_outside_the_terminal_has_no_tape() {
    for text in [
        "fixture history\nexec add f\n",
        "fixture history\nwrite f \"x\"\n",
        "fixture history\nconfig \"\"\n",
        "fixture remote\nasync-key f\n",
    ] {
        let parsed = script::parse(text).unwrap();
        let error = tape::tape("demo", &parsed).unwrap_err();
        assert!(error.contains("line 2"), "{text:?}: {error}");
    }
    let parsed = script::parse("key j\n").unwrap();
    assert!(tape::tape("demo", &parsed).unwrap_err().contains("fixture"));
}

// ---------------------------------------------------------- the binary itself

fn ferrit() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ferrit"))
}

fn temp(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ferrit-harness-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_binary_replays_a_script_and_dumps_its_frames() {
    let dir = temp("dump");
    let script = dir.join("demo.script");
    std::fs::write(
        &script,
        "fixture canonical\nkey 2\nsnapshot files\nsnapshot again\n",
    )
    .unwrap();
    let frames = dir.join("frames");
    let out = ferrit()
        .args(["--replay"])
        .arg(&script)
        .arg("--dump-frames")
        .arg(&frames)
        .args(["--size", "100x30"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let first = std::fs::read_to_string(frames.join("001-files.txt")).unwrap();
    assert!(
        first.contains("main.rs") && first.lines().count() == 30,
        "{first}"
    );
    assert!(frames.join("002-again.txt").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_binary_exits_non_zero_on_a_failed_script_and_prints_the_line_and_frame() {
    let dir = temp("fail");
    let script = dir.join("bad.script");
    std::fs::write(&script, "fixture canonical\nexpect-text \"never there\"\n").unwrap();
    let out = ferrit().arg("--replay").arg(&script).output().unwrap();
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(
        text.contains("line 2") && text.contains("never there"),
        "{text}"
    );
    assert!(text.contains("Status"), "the frame is printed: {text}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_binary_reports_a_script_that_does_not_parse() {
    let dir = temp("parse");
    let script = dir.join("bad.script");
    std::fs::write(&script, "fixture canonical\nfrobnicate\n").unwrap();
    let out = ferrit().arg("--replay").arg(&script).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("line 2"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_binary_builds_a_fixture_and_prints_where_and_makes_a_tape() {
    let dir = temp("fx");
    let out = ferrit()
        .args(["--fixture", "history", "--into"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(out.status.success());
    let printed = String::from_utf8(out.stdout).unwrap();
    assert!(Path::new(printed.trim()).join("f").exists(), "{printed}");

    let script = dir.join("demo.script");
    std::fs::write(&script, "fixture canonical\nkey 2\n").unwrap();
    let out = ferrit().arg("--tape").arg(&script).output().unwrap();
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .starts_with("Output test/shots/demo.gif")
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_harness_flags_are_hidden_from_help() {
    let out = ferrit().arg("--help").output().unwrap();
    let help = String::from_utf8_lossy(&out.stdout).to_lowercase();
    for word in ["replay", "fixture", "dump-frames", "tape"] {
        assert!(!help.contains(word), "--help mentions {word}:\n{help}");
    }
}

// ------------------------------------------------------------------ gen-tapes

#[test]
fn gen_tapes_writes_a_tape_per_replayable_script_and_names_why_it_skips_the_rest() {
    let out = temp("tapes");
    let result = Command::new("sh")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("test/gen-tapes.sh"))
        .env("FERRIT_BIN", env!("CARGO_BIN_EXE_ferrit"))
        .env("TAPES_DIR", &out)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let listing = String::from_utf8_lossy(&result.stdout);

    // Pure key-and-look flows have tapes ...
    for name in [
        "10-layout",
        "20-status-files",
        "30-diff",
        "35-scroll",
        "40-stage",
    ] {
        assert!(
            out.join(format!("{name}.tape")).exists(),
            "{name} should have a tape:\n{listing}"
        );
        assert!(listing.contains(&format!("tape: {name}")), "{listing}");
    }
    // ... and the ones that change the world from outside the terminal do not.
    for name in ["70-remote", "90-rewrite", "110-keymap"] {
        assert!(
            !out.join(format!("{name}.tape")).exists(),
            "{name} cannot have a tape"
        );
        let line = listing
            .lines()
            .find(|l| l.starts_with(&format!("skip: {name}")))
            .unwrap_or_default();
        assert!(line.contains("outside the terminal"), "{name}: {listing}");
    }
    let tape = std::fs::read_to_string(out.join("10-layout.tape")).unwrap();
    assert!(
        tape.starts_with("Output test/shots/10-layout.gif"),
        "{tape}"
    );
    std::fs::remove_dir_all(&out).unwrap();
}
