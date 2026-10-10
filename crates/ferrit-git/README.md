# ferrit-git

Concrete Git adapters for ferrit. This crate owns `git2`, Git subprocesses,
credential prompting, SSH config discovery and the in-memory test adapter.

The application depends on the ports and models from `ferrit-domain`; this crate
provides the real `Repo` implementation.
