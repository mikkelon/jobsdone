//! Raw mode, alternate screen, mouse capture while the `mouse` setting
//! asks for it, the panic hook, and the event loop with its 250 ms tick.
//!
//! Events go in and frames come out. This module never sees time: a tick
//! is an action like any other.

use std::io::{self, Stdout, Write};
use std::panic;
use std::process::{Command, Stdio};
use std::time::Duration;

use crossterm::event::{
    self, DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, Event, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
mod spelling_backend;
use spelling_backend::SpellingBackend;
use tracing::error;

use crate::app::{App, Flow};
use crate::input::{self, Action};

/// Every timeout is a tick (STACK.md section 2).
const TICK: Duration = Duration::from_millis(250);

/// Enters raw mode, installs the panic hook, loops until `Flow::Quit`,
/// restores the terminal.
pub fn run(mut app: App) -> io::Result<()> {
    // The `mouse` setting decides whether the program is handed the
    // mouse at all, from the first frame on.
    let mut mouse = app.settings().mouse();
    enter(mouse)?;
    install_panic_hook();

    let mut terminal = Terminal::new(SpellingBackend::new(io::stdout()))?;
    let outcome = go_round(&mut terminal, &mut app, &mut mouse);

    // The terminal is restored whether or not the loop ended well.
    if let Err(error) = leave() {
        error!(%error, "the terminal could not be restored");
    }
    outcome
}

fn go_round(
    terminal: &mut Terminal<SpellingBackend<Stdout>>,
    app: &mut App,
    mouse: &mut bool,
) -> io::Result<()> {
    loop {
        let mut layout = None;
        {
            let view = &*app;
            terminal.draw(|frame| layout = Some(crate::ui::draw(view, frame)))?;
        }
        if let Some(layout) = layout {
            app.set_layout(layout);
        }

        let action = if event::poll(TICK)? {
            let event = event::read()?;
            // A terminal the mouse has been handed back to sends no mouse
            // events. One that sends them anyway, a test driver writing
            // them straight into the pane, is answered the same way.
            if let Event::Paste(text) = event {
                app.paste(Ok(text));
                None
            } else if !*mouse && matches!(event, Event::Mouse(_)) {
                None
            } else {
                let action = input::action_for(&event, app.key_context());
                if action.is_none()
                    && let Some(key) = input::pressed(&event)
                {
                    app.unbound(&key);
                }
                action
            }
        } else {
            Some(Action::Tick)
        };

        if let Some(action) = action {
            match app.update(action) {
                Flow::Quit => return Ok(()),
                Flow::CopyTask(text) => app.copied_task(copy_to_clipboard(&text)),
                Flow::CopyNote(text) => app.copied_note(copy_to_clipboard(&text)),
                Flow::CopySelection(text) => app.copied_selection(false, copy_to_clipboard(&text)),
                Flow::CutSelection(text) => app.copied_selection(true, copy_to_clipboard(&text)),
                Flow::ReadClipboard => app.paste(read_from_clipboard()),
                Flow::Continue => {}
            }
            // The setting takes effect at once, whether it was changed on
            // the settings page or by another window a tick picked up.
            if app.settings().mouse() != *mouse {
                *mouse = !*mouse;
                take_the_mouse(*mouse)?;
            }
        }
    }
}

/// Use the desktop clipboard, including when the app runs inside tmux.
/// Text goes through stdin so prompts are never shell code or arguments.
fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let (program, args): (&str, &[&str]) = if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        ("wl-copy", &["--type", "text/plain;charset=utf-8"])
    } else if std::env::var_os("DISPLAY").is_some() {
        ("xclip", &["-selection", "clipboard", "-in"])
    } else {
        return Err("Could not copy: no desktop clipboard is available.".to_owned());
    };
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not copy: {program}: {error}"))?;
    let written = child
        .stdin
        .take()
        .expect("piped clipboard input")
        .write_all(text.as_bytes());
    let status = child.wait();
    written.map_err(|error| format!("Could not copy: {error}"))?;
    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => Err(format!("Could not copy: {program} failed.")),
        Err(error) => Err(format!("Could not copy: {error}")),
    }
}

fn read_from_clipboard() -> Result<String, String> {
    let (program, args): (&str, &[&str]) = if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        ("wl-paste", &["--no-newline"])
    } else if std::env::var_os("DISPLAY").is_some() {
        ("xclip", &["-selection", "clipboard", "-out"])
    } else {
        return Err("Could not paste: no desktop clipboard is available.".to_owned());
    };
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("Could not paste: {program}: {error}"))?;
    if !output.status.success() {
        return Err(format!("Could not paste: {program} failed."));
    }
    String::from_utf8(output.stdout).map_err(|error| format!("Could not paste: {error}"))
}

fn enter(mouse: bool) -> io::Result<()> {
    enable_raw_mode()?;
    if let Err(error) = enter_screen(&mut io::stdout()) {
        let _ = disable_raw_mode();
        return Err(error);
    }
    if let Err(error) = take_the_mouse(mouse) {
        let _ = leave_screen(&mut io::stdout());
        let _ = disable_raw_mode();
        return Err(error);
    }
    Ok(())
}

fn enter_screen(writer: &mut impl Write) -> io::Result<()> {
    execute!(
        writer,
        EnterAlternateScreen,
        EnableFocusChange,
        EnableBracketedPaste,
        // Let supporting terminals distinguish Shift+Enter from Enter.
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    )
}

/// Whether the terminal hands its mouse events to the program. With them
/// off the terminal's own selection and scrollback work again, which is
/// the whole of what the `mouse` setting buys (DOMAIN.md section 19).
fn take_the_mouse(taking: bool) -> io::Result<()> {
    if taking {
        execute!(io::stdout(), EnableMouseCapture)
    } else {
        execute!(io::stdout(), DisableMouseCapture)
    }
}

fn leave() -> io::Result<()> {
    let screen = leave_screen(&mut io::stdout());
    let raw = disable_raw_mode();
    screen.and(raw)
}

fn leave_screen(writer: &mut impl Write) -> io::Result<()> {
    execute!(
        writer,
        PopKeyboardEnhancementFlags,
        DisableFocusChange,
        DisableMouseCapture,
        DisableBracketedPaste,
        LeaveAlternateScreen
    )
}

/// A crash must never leave the terminal in raw mode, or the shell it
/// returns to is unusable.
fn install_panic_hook() {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = leave();
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_entry_and_exit_frame_terminal_paste() {
        let mut entered = Vec::new();
        enter_screen(&mut entered).unwrap();
        assert!(entered.windows(8).any(|bytes| bytes == b"\x1b[?2004h"));
        assert!(entered.windows(5).any(|bytes| bytes == b"\x1b[>1u"));

        let mut left = Vec::new();
        leave_screen(&mut left).unwrap();
        assert!(left.windows(8).any(|bytes| bytes == b"\x1b[?2004l"));
        assert!(left.windows(5).any(|bytes| bytes == b"\x1b[<1u"));
    }
}
