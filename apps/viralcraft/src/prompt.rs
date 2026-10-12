//! Questions to the user go to stderr; answers come from stdin.

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;

/// Asks for a path. An empty answer or closed stdin is no answer.
pub fn ask_path(question: &str) -> Option<PathBuf> {
    let answer = ask(question)?;
    (!answer.is_empty()).then(|| PathBuf::from(answer))
}

/// Asks a yes/no question; anything but y/yes is no.
pub fn confirm(question: &str) -> bool {
    matches!(ask(question).as_deref(), Some("y" | "Y" | "yes"))
}

/// Asks a [Y/n] question: Enter is yes, anything but y/yes is no. Closed
/// stdin is no, so nothing installs when no one is at the terminal.
pub fn confirm_default_yes(question: &str) -> bool {
    matches!(
        ask(question).map(|a| a.to_lowercase()).as_deref(),
        Some("" | "y" | "yes")
    )
}

/// A free answer, lower-cased; `None` for an empty answer or closed stdin.
pub fn answer(question: &str) -> Option<String> {
    ask(question)
        .map(|a| a.trim().to_lowercase())
        .filter(|a| !a.is_empty())
}

fn ask(question: &str) -> Option<String> {
    eprint!("{question} ");
    let _ = io::stderr().flush();
    let mut answer = String::new();
    match io::stdin().lock().read_line(&mut answer) {
        Ok(0) | Err(_) => {
            eprintln!();
            None
        }
        Ok(_) => Some(answer.trim().to_string()),
    }
}

/// A service key: from the environment, else asked for with hidden typing
/// when someone is at the terminal. Used for this run only, never stored.
/// `None` when it is not set and no one can be asked.
pub fn api_key(var: &str) -> Option<String> {
    if let Some(key) = std::env::var(var).ok().filter(|k| !k.trim().is_empty()) {
        return Some(key);
    }
    if !io::stdin().is_terminal() {
        return None;
    }
    // Pasting shows nothing, which looks broken unless the user is told.
    let question = format!(
        "{var} is not set. Paste your key and press Enter.\n\
         The key stays hidden: nothing will show while you paste. It is used for this run only, never saved.\n\
         Key: "
    );
    rpassword::prompt_password(question)
        .ok()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
}
