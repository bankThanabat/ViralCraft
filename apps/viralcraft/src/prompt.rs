//! Questions to the user go to stderr; answers come from stdin.

use std::io::{self, BufRead, Write};
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
