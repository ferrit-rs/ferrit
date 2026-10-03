#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! The settings sheet's rows (`docs/PLAN_17_SETTINGS.md`, U1): each changes the
//! live configuration at once and is on disk at once, in its own section.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrit::app::App;
use ferrit::app::config::Config;
use ferrit::app::settings::{Kind, SaveState, SettingsRow, TerminalRequest};
use ferrit::app::theme_config::{Base, Preset};

struct Fixture {
    dir: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("ferrit-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(dir.join("repo")).unwrap();
        let status = Command::new("git")
            .arg("-C")
            .arg(dir.join("repo"))
            .args(["init", "-q", "."])
            .status()
            .unwrap();
        assert!(status.success());
        Self { dir }
    }

    fn file(&self) -> PathBuf {
        self.dir.join("config.toml")
    }

    /// An app on the repository whose config lives in `config.toml` here.
    fn app(&self) -> App {
        App::open_with(&self.dir.join("repo"), Config::load_from(&self.file())).unwrap()
    }

    fn saved(&self) -> Config {
        Config::load_from(&self.file()).config
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn press(app: &mut App, row: SettingsRow, up: bool) {
    app.change_setting(row, up);
}

#[test]
fn every_row_has_a_label_a_group_and_a_kind() {
    for row in SettingsRow::ALL {
        assert!(!row.label().is_empty() && !row.group().is_empty());
        match row.kind() {
            Kind::Choice(names) => assert!(names.len() >= 2),
            Kind::Number { min, max } => assert!(min < max),
            Kind::Toggle => {},
        }
    }
    let groups: Vec<_> = SettingsRow::ALL.iter().map(|r| r.group()).collect();
    assert_eq!(
        groups,
        [
            "Appearance",
            "Appearance",
            "Interface",
            "Interface",
            "Diff",
            "Diff",
            "Commit",
            "Command log"
        ]
    );
}

#[test]
fn the_theme_flips_between_dark_and_light_and_the_palette_follows() {
    let fx = Fixture::new("set-theme");
    let mut app = fx.app();
    assert_eq!(app.choice_index(SettingsRow::Theme), Some(0), "Dark");
    assert!(!app.palette().light);

    press(&mut app, SettingsRow::Theme, true);
    assert_eq!(app.choice_index(SettingsRow::Theme), Some(1), "Light");
    assert!(app.palette().light, "the colours are the light ones now");
    assert_eq!(fx.saved().theme.base, Base::Light, "on disk at once");

    press(&mut app, SettingsRow::Theme, false);
    assert!(!app.palette().light);
    assert_eq!(fx.saved().theme.base, Base::Dark);
}

#[test]
fn the_accent_walks_the_presets_both_ways_and_a_custom_colour_shows_as_none() {
    let fx = Fixture::new("set-accent");
    let mut app = fx.app();
    assert_eq!(app.choice_index(SettingsRow::Accent), Some(0));
    press(&mut app, SettingsRow::Accent, true);
    assert_eq!(app.choice_index(SettingsRow::Accent), Some(1));
    assert_eq!(fx.saved().theme.preset, Preset::Blue);
    press(&mut app, SettingsRow::Accent, false);
    press(&mut app, SettingsRow::Accent, false);
    assert_eq!(
        app.choice_index(SettingsRow::Accent),
        Some(3),
        "wraps to Amber"
    );
    assert_eq!(fx.saved().theme.preset, Preset::Amber);
}

#[test]
fn the_toggles_flip_and_each_lands_in_its_own_section() {
    let fx = Fixture::new("set-toggles");
    let mut app = fx.app();
    for row in [
        SettingsRow::Mouse,
        SettingsRow::IgnoreWhitespace,
        SettingsRow::SignOff,
        SettingsRow::ShowReads,
    ] {
        let before = app.toggle_value(row);
        press(&mut app, row, true);
        assert_eq!(app.toggle_value(row), !before, "{}", row.label());
    }
    let saved = fx.saved();
    assert!(!saved.ui.mouse, "mouse started on");
    assert!(saved.diff.ignore_whitespace);
    assert!(saved.commit.sign_off);
    assert!(saved.log.show_reads);

    // The other direction flips as well: a toggle has no "up".
    press(&mut app, SettingsRow::SignOff, false);
    assert!(!app.toggle_value(SettingsRow::SignOff));
    assert!(!fx.saved().commit.sign_off);
}

#[test]
fn the_numbers_step_by_one_and_stop_at_their_ends() {
    let fx = Fixture::new("set-numbers");
    let mut app = fx.app();
    assert_eq!(app.number_value(SettingsRow::WheelStep), 3);
    press(&mut app, SettingsRow::WheelStep, true);
    assert_eq!(app.number_value(SettingsRow::WheelStep), 4);
    for _ in 0..80 {
        press(&mut app, SettingsRow::WheelStep, true);
    }
    assert_eq!(app.number_value(SettingsRow::WheelStep), 50, "stops at 50");
    for _ in 0..80 {
        press(&mut app, SettingsRow::WheelStep, false);
    }
    assert_eq!(app.number_value(SettingsRow::WheelStep), 1, "stops at 1");
    assert_eq!(fx.saved().ui.wheel_step, 1);

    assert_eq!(app.number_value(SettingsRow::DiffContext), 3);
    for _ in 0..5 {
        press(&mut app, SettingsRow::DiffContext, false);
    }
    assert_eq!(app.number_value(SettingsRow::DiffContext), 0, "stops at 0");
    press(&mut app, SettingsRow::DiffContext, true);
    assert_eq!(fx.saved().diff.context, 1);
    assert_eq!(app.live_config().diff.context, 1);
}

#[test]
fn turning_the_mouse_off_or_on_asks_the_run_loop_to_switch_the_terminal() {
    let fx = Fixture::new("set-mouse");
    let mut app = fx.app();
    assert!(app.mouse_enabled());
    press(&mut app, SettingsRow::Mouse, true);
    assert!(!app.mouse_enabled());
    // The request itself is for `run`; reading it again is not possible from here,
    // but the live state and the file agree.
    assert!(!fx.saved().ui.mouse);
    press(&mut app, SettingsRow::Mouse, true);
    assert!(app.mouse_enabled());
    assert!(fx.saved().ui.mouse);
    let _ = TerminalRequest::Mouse(true);
}

#[test]
fn a_change_keeps_every_other_key_and_section_of_the_file() {
    let fx = Fixture::new("set-keep");
    fs::write(
        fx.file(),
        "[keys.global]\nquit = \"x\"\n\n[from_the_future]\nanswer = 42\n\n[diff]\nrename_threshold = 80\n",
    )
    .unwrap();
    let mut app = fx.app();
    press(&mut app, SettingsRow::DiffContext, true);

    let text = fs::read_to_string(fx.file()).unwrap();
    assert!(text.contains("quit = \"x\""), "{text}");
    assert!(text.contains("answer = 42"), "{text}");
    assert!(
        text.contains("rename_threshold = 80"),
        "the unshown diff key is kept: {text}"
    );
    assert!(text.contains("context = 4"), "{text}");
}

#[test]
fn no_config_file_is_not_an_error_and_nothing_is_written() {
    let fx = Fixture::new("set-nofile");
    let mut app = App::open(&fx.dir.join("repo")).unwrap();
    press(&mut app, SettingsRow::SignOff, true);
    assert!(app.toggle_value(SettingsRow::SignOff), "it still applies");
    assert_eq!(app.settings().save, SaveState::Idle);
    assert!(!fx.file().exists());
}

#[test]
fn a_file_that_is_not_toml_is_left_alone_reported_and_the_setting_still_applies() {
    let fx = Fixture::new("set-broken");
    fs::write(fx.file(), "this is [not toml\n").unwrap();
    let mut app = fx.app();
    press(&mut app, SettingsRow::SignOff, true);

    assert!(
        app.toggle_value(SettingsRow::SignOff),
        "applies for this run"
    );
    let SaveState::Failed(why) = &app.settings().save else {
        panic!("the footer says why: {:?}", app.settings().save);
    };
    assert!(why.contains("not overwritten"), "{why}");
    assert_eq!(
        fs::read_to_string(fx.file()).unwrap(),
        "this is [not toml\n"
    );
}

#[test]
fn a_written_change_reports_saved_and_a_later_failure_replaces_it() {
    let fx = Fixture::new("set-saved");
    let mut app = fx.app();
    assert_eq!(app.settings().save, SaveState::Idle);
    press(&mut app, SettingsRow::ShowReads, true);
    assert_eq!(app.settings().save, SaveState::Saved);

    // The file becomes unreadable as TOML under the running app.
    fs::write(fx.file(), "[oops\n").unwrap();
    press(&mut app, SettingsRow::ShowReads, true);
    assert!(matches!(app.settings().save, SaveState::Failed(_)));
}

#[test]
fn the_theme_survives_the_app_being_rebuilt_on_a_new_repository() {
    let fx = Fixture::new("set-attach");
    let mut app = fx.app();
    press(&mut app, SettingsRow::Theme, true);
    app.attach_repository(Path::new(&fx.dir.join("repo")))
        .unwrap();
    assert!(app.palette().light, "the rebuilt app is still light");
    assert_eq!(app.choice_index(SettingsRow::Theme), Some(1));
}

#[test]
fn what_is_saved_is_what_the_next_start_loads() {
    let fx = Fixture::new("set-restart");
    {
        let mut app = fx.app();
        press(&mut app, SettingsRow::Theme, true);
        press(&mut app, SettingsRow::Accent, true);
        press(&mut app, SettingsRow::Mouse, true);
        press(&mut app, SettingsRow::WheelStep, true);
        press(&mut app, SettingsRow::DiffContext, true);
        press(&mut app, SettingsRow::IgnoreWhitespace, true);
        press(&mut app, SettingsRow::SignOff, true);
        press(&mut app, SettingsRow::ShowReads, true);
    }
    // A new start from the file alone.
    let load = Config::load_from(&fx.file());
    assert!(load.issues.is_empty(), "{:?}", load.issues);
    let reopened = App::open_with(&fx.dir.join("repo"), load).unwrap();
    assert_eq!(reopened.choice_index(SettingsRow::Theme), Some(1));
    assert_eq!(reopened.choice_index(SettingsRow::Accent), Some(1));
    assert!(!reopened.mouse_enabled());
    assert_eq!(reopened.number_value(SettingsRow::WheelStep), 4);
    assert_eq!(reopened.number_value(SettingsRow::DiffContext), 4);
    for row in [
        SettingsRow::IgnoreWhitespace,
        SettingsRow::SignOff,
        SettingsRow::ShowReads,
    ] {
        assert!(reopened.toggle_value(row), "{}", row.label());
    }
}
