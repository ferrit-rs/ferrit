//! Small credential-prompt rules shared by adapters and the TUI.

/// Whether typing for `prompt` should be hidden: passwords, passphrases and
/// PINs, not a username or an SSH host-key question.
#[must_use]
pub fn is_secret(prompt: &str) -> bool {
    let prompt = prompt.to_lowercase();
    ["password", "passphrase", "pin"]
        .iter()
        .any(|word| prompt.contains(word))
}
