//! The Hyprland window rule, which the program keeps for itself.
//!
//! Two settings are not the program's to obey: whether its window floats
//! and how big it is are the window manager's, and it is told about them
//! by a block in its own configuration. The block belongs to the program,
//! so the install script does not write it and the settings page can
//! change it without an install (STACK.md section 7).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::app::{Desktop, WindowSize};

#[cfg(test)]
mod tests;

/// The lines the block is between. They stay even when nothing is
/// between them, so that an uninstall still finds the block to remove.
const BEGIN: &str = "-- jobsdone: window (begin)";
const END: &str = "-- jobsdone: window (end)";

/// What the window says it is, which is what the rule matches on.
const APP_ID: &str = "org.omarchy.jobsdone";

/// Set in the environment of everything Hyprland starts, so it is how a
/// running Hyprland is recognised.
const SIGNATURE: &str = "HYPRLAND_INSTANCE_SIGNATURE";

/// What is said when there is no Hyprland to tell. The setting is kept
/// either way: a database carried to a machine that has one is a machine
/// that gets the window it was asked for.
const ABSENT: &str = "Hyprland is not here; the setting is kept for when it is.";

/// Hyprland as this program talks to it: one file, one reload, and
/// the dispatches that show a size being chosen.
pub struct Hyprland {
    /// `$XDG_CONFIG_HOME/hypr/bindings.lua`, or nothing when there is no
    /// home directory to find it under.
    bindings: Option<PathBuf>,
}

impl Hyprland {
    pub fn here() -> Hyprland {
        Hyprland {
            bindings: xdg::BaseDirectories::with_prefix("hypr")
                .get_config_home()
                .map(|dir| dir.join("bindings.lua")),
        }
    }
}

impl Desktop for Hyprland {
    /// A configuration directory to write into, or a Hyprland running to
    /// write it for. The directory alone counts, so that the rule can be
    /// put in place from a terminal on another compositor and be there at
    /// the next login.
    fn available(&self) -> bool {
        let configured = self
            .bindings
            .as_ref()
            .and_then(|path| path.parent().map(Path::is_dir))
            .unwrap_or(false);
        configured || env::var_os(SIGNATURE).is_some()
    }

    fn apply_window(&self, floating: bool, size: WindowSize) -> Result<(), String> {
        let Some(path) = self.bindings.as_ref().filter(|_| self.available()) else {
            return Err(ABSENT.to_owned());
        };
        let was = fs::read_to_string(path).unwrap_or_default();
        let now = with_block(&was, &block(floating, size));
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)
                .map_err(|error| format!("{} could not be made: {error}", dir.display()))?;
        }
        fs::write(path, now)
            .map_err(|error| format!("{} could not be written: {error}", path.display()))?;
        reload()
    }

    /// The rule is what the window opens at; this is the window it is in
    /// already. Nothing to dispatch to when Hyprland is not running,
    /// which is the same case the reload sits out.
    fn preview(&self, size: WindowSize) -> Result<(), String> {
        if env::var_os(SIGNATURE).is_none() {
            return Ok(());
        }
        hyprctl(&[
            "dispatch",
            "resizeactive",
            "exact",
            &size.width.to_string(),
            &size.height.to_string(),
        ])?;
        hyprctl(&["dispatch", "centerwindow"])?;
        Ok(())
    }
}

/// The block as it is written, markers included. A tiled window is the
/// absence of a rule rather than a rule of its own.
fn block(floating: bool, size: WindowSize) -> String {
    let mut lines = vec![BEGIN.to_owned()];
    if floating {
        lines.push(format!(
            "o.window(\"{APP_ID}\", {{ float = true, center = true, size = {{ {}, {} }} }})",
            size.width, size.height
        ));
    }
    lines.push(END.to_owned());
    lines.join("\n")
}

/// The file with the block in it: where the block already was, so that
/// rewriting it never moves anybody's own lines, and at the end
/// otherwise.
fn with_block(text: &str, block: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let begin = lines.iter().position(|line| line.trim() == BEGIN);
    let end = lines.iter().position(|line| line.trim() == END);

    let mut kept: Vec<&str> = match (begin, end) {
        (Some(begin), Some(end)) if end >= begin => {
            let mut kept = lines[..begin].to_vec();
            kept.extend(block.lines());
            kept.extend_from_slice(&lines[end + 1..]);
            kept
        }
        _ => {
            let mut kept = lines.clone();
            // A blank line between the block and whatever was written
            // above it.
            while kept.last().is_some_and(|line| line.trim().is_empty()) {
                kept.pop();
            }
            if !kept.is_empty() {
                kept.push("");
            }
            kept.extend(block.lines());
            kept
        }
    };
    kept.push("");
    kept.join("\n")
}

/// Hyprland reads its configuration again, and says whether it liked it.
/// Nothing is running to reload when the rule is written from another
/// session, which is not a failure.
fn reload() -> Result<(), String> {
    if env::var_os(SIGNATURE).is_none() {
        return Ok(());
    }
    hyprctl(&["reload"])?;
    let errors = hyprctl(&["configerrors"])?;
    let first = errors
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.contains("no errors"));
    match first {
        Some(error) => Err(format!("Hyprland did not take the rule: {error}")),
        None => Ok(()),
    }
}

fn hyprctl(arguments: &[&str]) -> Result<String, String> {
    let output = Command::new("hyprctl")
        .args(arguments)
        .output()
        .map_err(|error| format!("hyprctl {} could not be run: {error}", arguments.join(" ")))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
