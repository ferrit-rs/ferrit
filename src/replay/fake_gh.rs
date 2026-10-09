//! A fake `gh` for replay scripts (`docs/PLAN_15_CREATE_REMOTE.md`): a shell
//! script written next to the fixture, so a script can walk the whole creation
//! without ever reaching GitHub. It says it is installed and signed in, and on
//! `repo create` it makes a bare repository beside the fixture and adds it as
//! `origin`, which is what the real `gh` does with GitHub's URL.
//!
//! The runner installs it for every session: the replay never runs a real `gh`.

use std::path::Path;

use crate::app::App;
use crate::git::host::GhProgram;

/// What the fake does. `--version` and `auth status` succeed; `repo create`
/// reads its `--source` directory and its target, makes `created.git` next to
/// the script and prints the repository's web URL.
const SCRIPT: &str = "#!/bin/sh
here=$(dirname \"$0\")
case \"$1\" in
  --version) echo 'gh version 2.50.0 (fake)'; exit 0 ;;
  auth) exit 0 ;;
  repo)
    target=\"$3\"
    case \"$target\" in */*) path=\"$target\" ;; *) path=\"fake-user/$target\" ;; esac
    while [ $# -gt 0 ]; do [ \"$1\" = --source ] && src=\"$2\"; shift; done
    git init -q --bare \"$here/created.git\"
    git -C \"$src\" remote add origin \"$here/created.git\"
    git -C \"$src\" config \"url.$here/created.git.insteadOf\" \"git@replay-alias:$path.git\"
    echo \"https://github.com/$path\"
    exit 0 ;;
esac
exit 0
";

/// An ssh config with the GitHub alias the replay's form presets, so the
/// creation rewrites `origin` over it. The fake's `insteadOf` above sends a push
/// to that alias to the bare repository it made.
const SSH_CONFIG: &str = "Host replay-alias\n  HostName github.com\n  User git\n";

/// Write the fake and an ssh config into `root` and point `app` at them. Not unix: nothing is
/// installed, and the real `gh` stays the program (no script uses it there).
pub fn install(root: &Path, app: &mut App) {
    let ssh_config = root.join("ssh_config");
    if std::fs::write(&ssh_config, SSH_CONFIG).is_ok() {
        app.set_ssh_config_path(ssh_config);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = root.join("gh");
        if std::fs::write(&path, SCRIPT).is_ok()
            && std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).is_ok()
        {
            app.set_gh_program(GhProgram::new(path));
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (root, app, SCRIPT, GhProgram::default());
    }
}
