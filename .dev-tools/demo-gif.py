#!/usr/bin/env python3
"""Dev-only: record the README's demo GIF (`demo-ferrit.gif`) from a real ferrit.

Two short stories in one loop, shot as screenshots of ferrit in a Terminal.app
window under tmux (`tui-shot.sh`), cropped to the terminal's cells, and joined with
PIL (no vhs or ffmpeg needed):

  1. from nothing: an empty folder, `i` (git init), `G` (create the repository on
     GitHub, first commit, origin, push).
  2. every day: a repository with history: a diff, stage, commit, the dashboard,
     the settings, the Light theme.

Nothing real is touched or shown. `gh` is a stand-in script (it makes a bare
repository beside itself instead of reaching GitHub), the people are invented, the
home folder is a throwaway one (so no user name and no config path leaks into a
frame), and the title bar and the tmux status bar (user and host name) are cropped
away. Because `gh` is a stand-in, no repository is created on GitHub.

Needs: Terminal.app, tmux, Screen Recording permission for the app hosting this
shell (`__SOP/visual-verify.md`), a built `target/debug/ferrit`, and Pillow.

    cargo build && python3 .dev-tools/demo-gif.py
    python3 .dev-tools/demo-gif.py --assemble-only   # only re-join the last screenshots
"""
import datetime
import os
import pathlib
import random
import shutil
import subprocess
import sys
import time

from PIL import Image, ImageStat

ROOT = pathlib.Path(__file__).resolve().parent.parent
FERRIT = ROOT / "target/debug/ferrit"
DEMO = pathlib.Path("/private/tmp/demo")  # short and neutral: it shows in a frame
SHOTS = ROOT / ".verify-shots/gif"
OUT = ROOT / "demo-ferrit.gif"
COLS, ROWS, WIDTH = 120, 34, 1100

# (frame, seconds on screen)
PLAN = [
    ("a01_welcome", 2.2), ("a02_init_question", 2.2), ("a03_panes", 1.6),
    ("a04_form", 2.4), ("a05_confirm", 2.8), ("a07_done", 3.2),
    ("b02_select", 2.4), ("b03_stage", 1.8), ("b04_commit", 2.4),
    ("b05_committed", 1.8), ("b07_dashboard_ready", 3.4),
    ("b09_settings", 2.4), ("b10_light_sheet", 2.6), ("b11_light_panes", 2.8),
]

GH = """#!/bin/sh
# A stand-in for gh, for a demo recording: it never reaches GitHub.
here=$(dirname "$0")
case "$1" in
  --version) echo 'gh version 2.50.0'; exit 0 ;;
  auth) exit 0 ;;
  repo)
    target="$3"
    case "$target" in */*) path="$target" ;; *) path="ada-rivera/$target" ;; esac
    while [ $# -gt 0 ]; do [ "$1" = --source ] && src="$2"; shift; done
    git init -q --bare "$here/created.git"
    git -C "$src" remote add origin "$here/created.git"
    git -C "$src" config "url.$here/created.git.insteadOf" "git@github.com-work:$path.git"
    echo "https://github.com/$path"
    exit 0 ;;
esac
exit 0
"""


def setup():
    shutil.rmtree(DEMO, ignore_errors=True)
    (DEMO / "bin").mkdir(parents=True)
    (DEMO / "home/.ssh").mkdir(parents=True)
    (DEMO / "my-project").mkdir()
    (DEMO / "home/.gitconfig").write_text(
        "[user]\n\tname = Ada Rivera\n\temail = ada@acme.example\n[init]\n\tdefaultBranch = main\n"
    )
    (DEMO / "home/.ssh/config").write_text("Host github.com-work\n  HostName github.com\n  User git\n")
    gh = DEMO / "bin/gh"
    gh.write_text(GH)
    gh.chmod(0o755)
    make_history(DEMO / "acme-api")


def make_history(repo):
    """139 conventional commits over 90 days by three invented people, then a
    staged change, an unstaged one and an untracked file."""
    repo.mkdir()

    def git(*a, env=None):
        e = dict(os.environ, GIT_CONFIG_GLOBAL="/dev/null", GIT_CONFIG_SYSTEM="/dev/null")
        e.update(env or {})
        subprocess.run(["git", "-C", str(repo), *a], env=e, check=True, capture_output=True)

    git("init", "-q", "-b", "main")
    random.seed(7)
    authors = [("Ada Rivera", "ada@acme.example"), ("Sam Okoye", "sam@acme.example"),
               ("Mina Park", "mina@acme.example")]
    kinds = ["feat", "fix", "docs", "test", "refactor", "chore"]
    files = ["src/main.rs", "src/routes.rs", "src/auth.rs", "src/db.rs", "src/config.rs",
             "tests/api.rs", "README.md", "docs/design.md", "Cargo.toml"]
    subjects = ["add pagination", "handle empty body", "cache user lookups", "tighten token expiry",
                "split the router", "document the endpoints", "retry on lost connection",
                "remove dead code", "validate the payload", "speed up startup",
                "log request ids", "rename the config keys"]
    now = datetime.datetime.now()
    n = 0
    for day in range(89, -1, -1):
        for _ in range(random.choices([0, 1, 2, 3, 5], [3, 4, 3, 2, 1])[0]):
            a = random.choices(authors, [5, 3, 2])[0]
            k = random.choices(kinds, [10, 6, 4, 3, 2, 2])[0]
            f = random.choices(files, [8, 7, 6, 6, 3, 5, 3, 2, 2])[0]
            p = repo / f
            p.parent.mkdir(parents=True, exist_ok=True)
            with open(p, "a") as fh:
                fh.write(f"line {n}\n")
            n += 1
            when = (now - datetime.timedelta(days=day, hours=random.randint(0, 9))).replace(microsecond=0).isoformat()
            git("add", "-A")
            git("commit", "-q", "-m", f"{k}: {random.choice(subjects)}", env={
                "GIT_AUTHOR_NAME": a[0], "GIT_AUTHOR_EMAIL": a[1], "GIT_COMMITTER_NAME": a[0],
                "GIT_COMMITTER_EMAIL": a[1], "GIT_AUTHOR_DATE": when, "GIT_COMMITTER_DATE": when})
    for b in ["feat/pagination", "fix/token-expiry"]:
        git("branch", b)
    (repo / "src/routes.rs").open("a").write('pub fn health() -> &\'static str { "ok" }\n')
    git("add", "src/routes.rs")
    (repo / "src/auth.rs").open("a").write("// TODO: rotate signing keys\n")
    (repo / "docs/notes.md").write_text("notes\n")


def command(folder):
    return (f"cd {DEMO}/{folder} && HOME={DEMO}/home PATH='{DEMO}/bin':$PATH "
            f"FERRIT_NO_GRAPHICS=1 {FERRIT}")


def shoot(session, cmd, name, *keys, tries=4):
    """One screenshot. A shot taken after the window sat idle can come out as a
    blank dark window (the app is fine): take it again."""
    env = dict(os.environ, FERRIT_SHOT_SESSION=session, FERRIT_SHOT_CMD=cmd)
    path = SHOTS / f"{name}.png"
    for attempt in range(tries):
        subprocess.run([str(ROOT / ".dev-tools/tui-shot.sh"), f"gif/{name}", *(keys if attempt == 0 else [])],
                       env=env, capture_output=True)
        time.sleep(0.8)
        im = Image.open(path).convert("L")
        w, h = im.size
        if ImageStat.Stat(im.crop((int(w * .1), int(h * .2), int(w * .9), int(h * .8)))).stddev[0] > 12:
            return
        env["FERRIT_SHOT_CMD"] = "x"
        subprocess.run(["tmux", "refresh-client", "-t", session], capture_output=True)
        time.sleep(1.2)
    sys.exit(f"{name}: blank after {tries} tries")


def resize(session):
    wid = (ROOT / f".verify-shots/.terminal_window_id.{session}").read_text().strip()
    subprocess.run(["osascript", "-e", f'tell application "Terminal" to set number of columns of window id {wid} to {COLS}',
                    "-e", f'tell application "Terminal" to set number of rows of window id {wid} to {ROWS}'],
                   capture_output=True)
    time.sleep(2)


def find(session, text):
    screen = subprocess.run(["tmux", "capture-pane", "-t", session, "-p"], capture_output=True, text=True).stdout
    for row, line in enumerate(screen.split("\n"), 1):
        if text in line:
            return line.index(text) + 1, row
    sys.exit(f"{text!r} is not on screen")


def click(session, text, shift=0):
    col, row = find(session, text)
    col += shift
    subprocess.run(["tmux", "send-keys", "-t", session, "-l", f"\x1b[<0;{col};{row}M\x1b[<0;{col};{row}m"])
    time.sleep(1.2)


def kill(*sessions):
    for s in sessions:
        subprocess.run(["tmux", "kill-session", "-t", s], capture_output=True)


def record():
    shutil.rmtree(SHOTS, ignore_errors=True)
    kill("demoA", "demoB")
    # 1. from nothing
    a = command("my-project")
    shoot("demoA", a, "a00_boot")
    resize("demoA")
    shoot("demoA", "x", "a01_welcome")
    shoot("demoA", "x", "a02_init_question", "i")
    shoot("demoA", "x", "a03_panes", "y")
    shoot("demoA", "x", "a04_form", "G")
    shoot("demoA", "x", "a05_confirm", "Enter")
    shoot("demoA", "x", "a06_creating", "y")
    time.sleep(3)
    shoot("demoA", "x", "a07_done")
    kill("demoA")
    # 2. every day
    b = command("acme-api")
    shoot("demoB", b, "b00_boot")
    resize("demoB")
    shoot("demoB", "x", "b01_panes")
    shoot("demoB", "x", "b02_select", "j", "j", "j", "j")
    shoot("demoB", "x", "b03_stage", "Space")
    shoot("demoB", "x", "b04_commit", "c", "text:feat: add a health check endpoint")
    shoot("demoB", "x", "b05_committed", "Enter")
    shoot("demoB", "x", "b06_dashboard", "D")
    time.sleep(3)
    shoot("demoB", "x", "b07_dashboard_ready")
    shoot("demoB", "x", "b08_closed", "Escape")
    click("demoB", "Ada Rivera", 2)  # a click on the author's name: the settings
    shoot("demoB", "x", "b09_settings")
    click("demoB", "( ) Light", 1)
    shoot("demoB", "x", "b10_light_sheet")
    subprocess.run(["tmux", "send-keys", "-t", "demoB", "Escape"])
    time.sleep(1)
    shoot("demoB", "x", "b11_light_panes")
    kill("demoB")


def assemble():
    frames, durations = [], []
    for name, seconds in PLAN:
        im = Image.open(SHOTS / f"{name}.png").convert("RGB")
        w, h = im.size
        # Only the terminal's cells: not the title bar (user name) nor tmux's bar (host name).
        crop = im.crop((round(w * 88 / 1856), round(h * 130 / 1186), round(w * 1768 / 1856), round(h * 1054 / 1186)))
        frames.append(crop.resize((WIDTH, round(crop.height * WIDTH / crop.width)), Image.LANCZOS))
        durations.append(int(seconds * 1000))
    # A palette of its own per frame (256 colours): the settings' colour picker is a
    # rainbow that a palette shared by every frame cannot hold.
    quantized = [f.quantize(colors=256, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE)
                 for f in frames]
    quantized[0].save(OUT, save_all=True, append_images=quantized[1:], duration=durations,
                      loop=0, optimize=True, disposal=1)
    print(f"{OUT}: {OUT.stat().st_size / 1e6:.2f} MB, {len(frames)} frames")


if __name__ == "__main__":
    if "--assemble-only" in sys.argv:  # join the screenshots already in .verify-shots/gif
        assemble()
        sys.exit()
    if not FERRIT.exists():
        sys.exit("build first: cargo build")
    setup()
    record()
    assemble()
    shutil.rmtree(DEMO, ignore_errors=True)
