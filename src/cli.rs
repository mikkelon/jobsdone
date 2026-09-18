//! The non-interactive command line: the grammar, the operation request it
//! becomes, and the two ways an answer is written.
//!
//! Nothing here touches the world. `parse` turns arguments into a `Plan`,
//! `present` turns the service's answer into output, and `main.rs` does the
//! opening, the calling and the printing in between. That split is what lets
//! every shape of the command line be a test rather than a run.

pub mod help;
pub mod parse;
pub mod render;
pub mod request;

#[cfg(test)]
mod tests;

use std::path::PathBuf;

use serde_json::{Map, Value};

use crate::domain::DateOrder;

pub use parse::parse;
pub use request::{Body, Source};

/// The bundled agent skill, printed by `--skill`. It is compiled in so that
/// the answer needs no file, no database and no directory that exists.
pub const SKILL: &str = include_str!("../skills/jobsdone/SKILL.md");

/// The version of the machine response. Errors carry it too, so a reader
/// that only ever sees failures still knows what it is reading.
pub const SCHEMA_VERSION: u64 = 1;

/// How an answer is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Format {
    /// Lists and summaries meant to be read.
    #[default]
    Text,
    /// The versioned envelope.
    Json,
}

/// What a command line asks the program to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plan {
    /// No command on the line: open the app, on the notes page when
    /// `--notes` asks for it.
    Run {
        notes: bool,
    },
    /// `desktop`, with the window the flags asked for. `None` where a flag
    /// was left off and the setting stands.
    Desktop {
        floating: Option<bool>,
        size: Option<(i64, i64)>,
    },
    /// The usage text for the whole program or for one command.
    Help(String),
    Version,
    /// The bundled agent skill.
    Skill,
    /// One operation for the service.
    Operate(Operation),
}

/// One service operation, with what the adapter does with the answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Operation {
    /// The request body, tagged by `op`.
    pub request: serde_json::Value,
    /// Which reading of the answer the text format uses.
    pub view: render::View,
    /// What the adapter does once the service has answered.
    pub then: Then,
    /// Text the request still wants, which somebody who can read a file
    /// must fetch before the request is complete.
    pub body: Option<Body>,
    /// `--input`: a JSON body whose fields join the ones the flags set.
    pub input: Option<Source>,
}

impl Operation {
    /// The `op` the request carries, for messages about it.
    pub fn op(&self) -> &str {
        self.request
            .get("op")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
    }
}

/// Work the adapter does after the service has answered, which is only ever
/// the clipboard: nothing else about an operation lives outside the service.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Then {
    Nothing,
    /// Put the note the answer carries on the desktop clipboard.
    CopyNote,
}

/// A command line, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parsed {
    pub plan: Plan,
    pub format: Format,
    /// `--data-dir`, which stands in front of the environment override and
    /// the XDG directory both.
    pub data_dir: Option<PathBuf>,
}

/// Why an answer could not be given: the same three fields a service error
/// has, so that one printer serves both.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub code: String,
    pub message: String,
    pub exit_code: u8,
}

impl Failure {
    /// A command line that could not be read. Distinct from the program
    /// having run and failed, which is what every other code means.
    pub fn usage(message: impl Into<String>) -> Self {
        Self {
            code: "usage".to_owned(),
            message: message.into(),
            exit_code: 2,
        }
    }

    /// Something the adapter itself could not do: read a file, reach the
    /// clipboard, write to a closed pipe.
    pub fn runtime(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
            exit_code: 1,
        }
    }
}

/// The failure, written the way the format asks. Success goes to stdout and
/// this goes to stderr, so a script may read one and log the other.
pub fn report(failure: &Failure, format: Format) -> String {
    match format {
        Format::Text => format!("jobsdone: {}\n", failure.message),
        Format::Json => {
            let envelope = serde_json::json!({
                "schema_version": SCHEMA_VERSION,
                "ok": false,
                "error": { "code": failure.code, "message": failure.message },
            });
            format!("{envelope}\n")
        }
    }
}

/// An answer from something that is not a service operation, in the same
/// envelope, so that `--json` is one shape whatever was asked for. It
/// carries no `context`: nothing here loaded a model, so there is no
/// working day to report.
pub fn said(field: &str, text: &str, format: Format) -> String {
    match format {
        Format::Text => format!("{text}\n"),
        Format::Json => {
            let envelope = serde_json::json!({
                "schema_version": SCHEMA_VERSION,
                "ok": true,
                "data": { field: text },
            });
            format!("{envelope}\n")
        }
    }
}

/// The order the answer says dates are written in, where it says.
///
/// Which way round a date is written is a setting first and the locale
/// only where the setting defers to it, so an answer that carries the
/// resolved order is preferred to the one the service was handed.
pub fn written_order(envelope: &Value) -> Option<DateOrder> {
    let named = ["context", "data"].into_iter().find_map(|part| {
        envelope
            .get(part)
            .and_then(|part| part.get("date_order"))
            .and_then(Value::as_str)
    })?;
    match named {
        "month_first" => Some(DateOrder::MonthFirst),
        "day_first" => Some(DateOrder::DayFirst),
        _ => None,
    }
}

/// The note body a `note copy` is to put on the clipboard.
pub fn note_to_copy(envelope: &Value) -> Option<&str> {
    envelope.get("data")?.get("note")?.get("body")?.as_str()
}

/// The answer, written the way the format asks.
///
/// JSON is the envelope the service returned, unchanged and on one line: the
/// adapter adds nothing to it, so a reader parsing the response is parsing
/// what the service said. Text is a reading of the same envelope.
pub fn present(
    operation: &Operation,
    envelope: &serde_json::Value,
    format: Format,
    dates: DateOrder,
) -> Result<String, Failure> {
    match format {
        Format::Json => Ok(format!("{envelope}\n")),
        Format::Text => render::text(operation, envelope, dates),
    }
}

/// The request, with the text and the JSON body that had to be read put
/// into it.
///
/// The two ways of filling a field in cannot silently override each other:
/// a field the flags set and the body sets again is a command line that
/// says two things, and an `op` in the body that is not the command's is a
/// body pretending to be another operation. Both are refused.
pub fn complete(
    operation: &mut Operation,
    body: Option<String>,
    input: Option<String>,
) -> Result<(), Failure> {
    if let (Some(wanted), Some(text)) = (&operation.body, body) {
        let field = wanted.field;
        let fields = object(&mut operation.request)?;
        if fields.contains_key(field) {
            return Err(Failure::usage(format!(
                "{field} is given on the command line and read in as well"
            )));
        }
        fields.insert(field.to_owned(), Value::String(text));
    }

    let Some(input) = input else {
        return Ok(());
    };
    let given: Value = serde_json::from_str(&input)
        .map_err(|error| Failure::usage(format!("the input is not JSON: {error}")))?;
    let Value::Object(given) = given else {
        return Err(Failure::usage(
            "the input must be a JSON object of the operation's fields".to_owned(),
        ));
    };

    let op = operation.op().to_owned();
    let fields = object(&mut operation.request)?;
    for (key, value) in given {
        if key == "op" {
            if value.as_str() != Some(op.as_str()) {
                return Err(Failure::usage(format!(
                    "the input asks for {}, and the command is {op}",
                    value.as_str().unwrap_or("another operation")
                )));
            }
            continue;
        }
        if fields.contains_key(&key) {
            return Err(Failure::usage(format!(
                "{key} is given on the command line and in the input"
            )));
        }
        fields.insert(key, value);
    }
    Ok(())
}

fn object(request: &mut Value) -> Result<&mut Map<String, Value>, Failure> {
    request
        .as_object_mut()
        .ok_or_else(|| Failure::runtime("unreadable_request", "the request is not an object"))
}
