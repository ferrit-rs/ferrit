#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::Command;

use ferrit::git::askpass;

/// The whole path: a running ferrit answers, and ferrit started the way ssh
/// starts an askpass program (prompt as argument) prints that answer. One
/// test: the server is process-wide.
#[test]
fn a_helper_prints_the_answer_and_fails_when_cancelled() {
    let _server = askpass::serve(|prompt| {
        prompt
            .contains("passphrase")
            .then(|| "s3cret word".to_owned())
    })
    .unwrap();

    let mut git = Command::new("true");
    askpass::configure(&mut git);
    let socket = git
        .get_envs()
        .find(|(key, _)| *key == "FERRIT_ASKPASS_SOCKET")
        .and_then(|(_, value)| value)
        .expect("configure points the child at the socket")
        .to_owned();

    let ask = |prompt: &str| {
        Command::new(env!("CARGO_BIN_EXE_ferrit"))
            .arg(prompt)
            .env("FERRIT_ASKPASS_SOCKET", &socket)
            .output()
            .unwrap()
    };
    let answered = ask("Enter passphrase for key '/home/a/.ssh/id_ed25519': ");
    assert!(answered.status.success());
    assert_eq!(String::from_utf8_lossy(&answered.stdout), "s3cret word\n");

    let cancelled = ask("Username for 'https://example.com': ");
    assert!(!cancelled.status.success());
    assert!(cancelled.stdout.is_empty());
}
