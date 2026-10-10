//! ferrit as its own `SSH_ASKPASS` / `GIT_ASKPASS` helper, so an SSH
//! passphrase (or an HTTPS password, or a host-key "yes/no") asked during a
//! fetch, pull or push is answered in a popup instead of on a terminal the
//! TUI owns. See `docs/PLAN_9_REMOTE.md`, "Credentials".
//!
//! The running ferrit listens on a Unix socket. `configure` points the git
//! child at the ferrit binary itself; when ssh or git runs it as the askpass
//! program, `run_helper` (called first thing in `main`) forwards the prompt
//! over the socket, waits for the answer and prints it on stdout, which is
//! the askpass contract.

use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::OnceLock;
use std::thread;

/// Set on the git child so a ferrit run as askpass knows where to reach the
/// listening ferrit.
const SOCKET_ENV: &str = "FERRIT_ASKPASS_SOCKET";

/// Whether typing for `prompt` should be hidden: passwords, passphrases and
/// PINs, not a username or ssh's "continue connecting (yes/no)?".
#[must_use]
pub fn is_secret(prompt: &str) -> bool {
    let prompt = prompt.to_lowercase();
    ["password", "passphrase", "pin"]
        .iter()
        .any(|word| prompt.contains(word))
}

static SOCKET: OnceLock<PathBuf> = OnceLock::new();

/// The listening socket's directory, removed when dropped.
#[derive(Debug)]
pub struct Server {
    dir: PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Start answering askpass prompts with `ask` (`None` cancels). One server
/// per process; the socket sits in a private directory.
///
/// # Errors
/// The socket cannot be created, or a server already runs.
#[cfg(unix)]
pub fn serve<F>(ask: F) -> io::Result<Server>
where
    F: Fn(String) -> Option<String> + Send + 'static,
{
    use std::fs::DirBuilder;
    use std::os::unix::fs::DirBuilderExt;
    use std::os::unix::net::UnixListener;

    let dir = std::env::temp_dir().join(format!("ferrit-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    DirBuilder::new().mode(0o700).create(&dir)?;
    // Short names: a Unix socket path is capped at 104 bytes on macOS.
    let path = dir.join("s");
    let listener = UnixListener::bind(&path)?;
    let server = Server { dir };
    SOCKET
        .set(path)
        .map_err(|_| io::Error::other("askpass server already running"))?;
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = answer(&stream, &ask);
        }
    });
    Ok(server)
}

/// One prompt per connection: a line in, `OK <secret>` or `NO` out.
#[cfg(unix)]
fn answer<F>(stream: &std::os::unix::net::UnixStream, ask: &F) -> io::Result<()>
where
    F: Fn(String) -> Option<String>,
{
    let mut prompt = String::new();
    BufReader::new(stream).read_line(&mut prompt)?;
    let reply = match ask(prompt.trim_end().to_owned()) {
        Some(secret) => format!("OK {secret}\n"),
        None => "NO\n".to_owned(),
    };
    let mut writer = stream;
    writer.write_all(reply.as_bytes())
}

/// Not supported off Unix: there is no socket to listen on.
///
/// # Errors
/// Always.
#[cfg(not(unix))]
pub fn serve<F>(_ask: F) -> io::Result<Server>
where
    F: Fn(String) -> Option<String> + Send + 'static,
{
    let _ = (&SOCKET, thread::current);
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

/// Point a git child's credential prompts at this ferrit. A no-op until
/// `serve` has run (tests, the replay harness).
pub fn configure(cmd: &mut Command) {
    let (Some(socket), Ok(exe)) = (SOCKET.get(), std::env::current_exe()) else {
        return;
    };
    cmd.env(SOCKET_ENV, socket)
        .env("SSH_ASKPASS", &exe)
        .env("SSH_ASKPASS_REQUIRE", "force")
        .env("GIT_ASKPASS", &exe)
        .env("GIT_TERMINAL_PROMPT", "0");
}

/// When ferrit was started by ssh or git as askpass (the socket variable is
/// set): relay the prompt in `args`, print the answer, and say how to exit.
/// `None` means this is a normal start.
pub fn run_helper<I>(args: I) -> Option<ExitCode>
where
    I: Iterator<Item = String>,
{
    let socket = std::env::var_os(SOCKET_ENV)?;
    let prompt = args.collect::<Vec<_>>().join(" ").replace('\n', " ");
    Some(match relay(Path::new(&socket), &prompt) {
        Ok(Some(secret)) => {
            let _ = writeln!(io::stdout(), "{secret}");
            ExitCode::SUCCESS
        },
        _ => ExitCode::FAILURE,
    })
}

#[cfg(unix)]
fn relay(socket: &Path, prompt: &str) -> io::Result<Option<String>> {
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(socket)?;
    writeln!(stream, "{prompt}")?;
    let mut reply = String::new();
    BufReader::new(&stream).read_line(&mut reply)?;
    Ok(reply
        .trim_end_matches('\n')
        .strip_prefix("OK ")
        .map(str::to_owned))
}

#[cfg(not(unix))]
fn relay(_socket: &Path, _prompt: &str) -> io::Result<Option<String>> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

#[cfg(test)]
mod tests {
    use crate::git::askpass::is_secret;

    #[test]
    fn passphrases_and_passwords_are_hidden() {
        assert!(is_secret(
            "Enter passphrase for key '/home/a/.ssh/id_ed25519': "
        ));
        assert!(is_secret("Password for 'https://github.com': "));
    }

    #[test]
    fn usernames_and_host_key_questions_are_shown() {
        assert!(!is_secret("Username for 'https://github.com': "));
        assert!(!is_secret(
            "Are you sure you want to continue connecting (yes/no)? "
        ));
    }
}
