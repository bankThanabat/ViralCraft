//! `rebuild`: the optional step that remakes the format with the user's
//! own clips. This crate reads those clips; it never writes to them.

pub mod assets;
pub mod prompt;

pub use assets::{find_clips, resolve_folder, run, AssetsError, ASSETS_FILE, REBUILD_DIR};
pub use prompt::{PromptError, PROMPT_FILE};
