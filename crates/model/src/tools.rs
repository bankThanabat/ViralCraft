//! The one place ViralCraft starts external programs.

use std::io;
use std::path::Path;
use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("`{program}` is not installed or not on PATH; run `viralcraft doctor`")]
    NotFound { program: String },
    #[error("could not start `{program}`: {source}")]
    Spawn { program: String, source: io::Error },
    #[error("`{command}` failed ({status}): {stderr}")]
    Failed {
        command: String,
        status: String,
        stderr: String,
    },
}

/// Runs `program args...`, returns its stdout. On failure the error carries
/// the full command line and the tail of stderr.
pub fn run(program: &str, args: &[&str]) -> Result<String, ToolError> {
    exec(Command::new(program).args(args), program, args)
}

/// Like [`run`], with `dir` as the working directory. Use it to pass ffmpeg
/// relative paths inside filter arguments, where `:` and `,` in an absolute
/// path would need escaping.
pub fn run_in(dir: &Path, program: &str, args: &[&str]) -> Result<String, ToolError> {
    exec(
        Command::new(program).args(args).current_dir(dir),
        program,
        args,
    )
}

fn exec(command: &mut Command, program: &str, args: &[&str]) -> Result<String, ToolError> {
    let output = command.output().map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            ToolError::NotFound {
                program: program.to_string(),
            }
        } else {
            ToolError::Spawn {
                program: program.to_string(),
                source: e,
            }
        }
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: Vec<&str> = stderr.lines().rev().take(8).collect();
        return Err(ToolError::Failed {
            command: format!("{program} {}", args.join(" ")),
            status: output.status.to_string(),
            stderr: tail.into_iter().rev().collect::<Vec<_>>().join("\n"),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn stdout_is_returned_on_success() {
        let out = run("sh", &["-c", "printf hello"]).unwrap();
        assert_eq!(out, "hello");
    }

    #[cfg(unix)]
    #[test]
    fn failure_carries_the_command_and_its_stderr() {
        let err = run("sh", &["-c", "echo boom >&2; exit 3"]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("sh") && msg.contains("boom"), "{msg}");
    }

    #[cfg(unix)]
    #[test]
    fn run_in_uses_the_given_working_directory() {
        let dir = std::env::temp_dir().canonicalize().unwrap();
        let out = run_in(&dir, "pwd", &[]).unwrap();
        assert_eq!(Path::new(out.trim()).canonicalize().unwrap(), dir);
    }

    #[test]
    fn a_missing_program_is_reported_as_not_found() {
        let err = run("viralcraft-no-such-tool", &[]).unwrap_err();
        assert!(matches!(err, ToolError::NotFound { .. }));
    }
}
