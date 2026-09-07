//! The grammar: arguments in, a `Plan` out. Nothing here reads a file, a
//! clock or the database, so every form of the command line is a test.

use std::path::PathBuf;

use serde_json::{Map, Value};

use super::request::{self, Source, Spec, command_named, commands};
use super::{Failure, Format, Operation, Parsed, Plan, help};

/// The options the program takes wherever they appear, and whether each one
/// is followed by a value.
const GLOBAL: &[(&str, bool)] = &[
    ("--json", false),
    ("--format", true),
    ("--data-dir", true),
    ("--input", true),
    ("--help", false),
    ("-h", false),
    ("--version", false),
    ("-V", false),
    ("--skill", false),
];

/// The format a command line asks for, read on its own so that a command
/// line which cannot be parsed is still answered in the shape it asked for.
///
/// It reads only option tokens and stops at `--`, so the one thing it can
/// get wrong is a positional that is spelt exactly `--json`, which is a
/// title nobody types.
pub fn format_hint(arguments: &[String]) -> Format {
    let mut format = Format::Text;
    let mut tokens = arguments.iter();
    while let Some(token) = tokens.next() {
        if token == "--" {
            break;
        }
        let (name, attached) = split(token);
        match name {
            "--json" => format = Format::Json,
            "--format" => {
                let value = attached
                    .map(str::to_owned)
                    .or_else(|| tokens.next().cloned());
                if value.as_deref() == Some("json") {
                    format = Format::Json;
                }
            }
            _ => {}
        }
    }
    format
}

/// `--name=value` split into its two halves; anything else is the token and
/// nothing.
fn split(token: &str) -> (&str, Option<&str>) {
    match token.split_once('=') {
        Some((name, value)) if name.starts_with('-') => (name, Some(value)),
        _ => (token, None),
    }
}

/// One option as it was given. `parse` owns the type, so nothing else can
/// make one up; `request` only ever reads them through `single` and
/// `present`.
pub struct Given {
    name: String,
    value: Option<String>,
}

impl Given {
    fn name(&self) -> &str {
        &self.name
    }
}

/// A command line, read.
pub fn parse(arguments: &[String]) -> Result<Parsed, Failure> {
    let (words, tokens) = words_and_tokens(arguments);

    let spec = match words.as_slice() {
        [] => None,
        _ => Some(command_named(&words)?),
    };
    let allowed = spec.map(|spec| spec.options).unwrap_or(&[]);

    let mut options: Vec<Given> = Vec::new();
    let mut positionals: Vec<String> = Vec::new();
    let mut only_positionals = false;
    let mut tokens = tokens.into_iter().peekable();

    while let Some(token) = tokens.next() {
        if only_positionals {
            positionals.push(token);
            continue;
        }
        if token == "--" {
            only_positionals = true;
            continue;
        }
        if !token.starts_with('-') || token == "-" {
            positionals.push(token);
            continue;
        }

        let (name, attached) = split(&token);
        let takes_value = arity(name, allowed).ok_or_else(|| unknown_option(name, spec))?;
        let value = match (takes_value, attached) {
            (false, Some(_)) => {
                return Err(Failure::usage(format!("{name} takes no value")));
            }
            (false, None) => None,
            (true, Some(value)) => Some(value.to_owned()),
            (true, None) => Some(
                tokens
                    .next()
                    .ok_or_else(|| Failure::usage(format!("{name} wants a value after it")))?,
            ),
        };
        options.push(Given {
            name: name.to_owned(),
            value,
        });
    }

    build(spec, &options, positionals)
}

/// The leading words that name a command, and everything else.
///
/// A command is one or two bare words at the front of the line. A global
/// option may stand between them, because a global option may stand
/// anywhere; the first option that is not one ends the command's name,
/// since a command's own options only ever follow it.
///
/// Two words are tried before one, so that `history list` is the command
/// even though `history` alone names it too, and `help note` is `help`
/// with `note` as what it is about. A second word that turns out not to
/// belong to the name goes back to the front of the arguments.
fn words_and_tokens(arguments: &[String]) -> (Vec<String>, Vec<String>) {
    let mut words: Vec<String> = Vec::new();
    let mut tokens: Vec<String> = Vec::new();
    let mut arguments = arguments.iter().peekable();

    while words.len() < 2 {
        let Some(argument) = arguments.peek() else {
            break;
        };
        if *argument == "--" {
            break;
        }
        if argument.starts_with('-') && argument.as_str() != "-" {
            let (name, attached) = split(argument);
            let Some((_, takes_value)) = GLOBAL.iter().find(|(global, _)| *global == name) else {
                break;
            };
            tokens.push(arguments.next().expect("peeked").clone());
            if *takes_value
                && attached.is_none()
                && let Some(value) = arguments.next()
            {
                tokens.push(value.clone());
            }
            continue;
        }
        words.push(arguments.next().expect("peeked").clone());
    }

    if words.len() == 2 && command_named(&words).is_err() {
        let second = words.pop().expect("two words");
        if command_named(&words).is_ok() {
            tokens.push(second);
        } else {
            // Neither reading is a command. The pair is put back so that
            // the message names what was actually written.
            words.push(second);
        }
    }
    tokens.extend(arguments.cloned());
    (words, tokens)
}

/// Whether an option is followed by a value, or `None` if no such option
/// exists here.
fn arity(name: &str, allowed: &[(&str, bool)]) -> Option<bool> {
    GLOBAL
        .iter()
        .chain(allowed.iter())
        .find(|(option, _)| *option == name)
        .map(|(_, takes)| *takes)
}

fn unknown_option(name: &str, spec: Option<&'static Spec>) -> Failure {
    match spec {
        Some(spec) => Failure::usage(format!("{name} is not an option of `{}`", spec.name)),
        None => Failure::usage(format!("{name} is not an option")),
    }
}

/// The options and words, turned into what the program will do.
fn build(
    spec: Option<&'static Spec>,
    options: &[Given],
    positionals: Vec<String>,
) -> Result<Parsed, Failure> {
    let format = format_of(options)?;
    let data_dir = single(options, "--data-dir")?.map(PathBuf::from);

    // Help, version and the skill are answered before anything else, so that
    // they need no database and no command line that makes sense.
    if present(options, "--skill") {
        return Ok(Parsed {
            plan: Plan::Skill,
            format,
            data_dir,
        });
    }
    if present(options, "--help") || present(options, "-h") {
        return Ok(Parsed {
            plan: Plan::Help(help::for_command(spec)),
            format,
            data_dir,
        });
    }
    if present(options, "--version") || present(options, "-V") {
        return Ok(Parsed {
            plan: Plan::Version,
            format,
            data_dir,
        });
    }

    let Some(spec) = spec else {
        if let Some(first) = positionals.first() {
            return Err(Failure::usage(format!("{first} is not a command")));
        }
        // Nothing on the command line opens the app. `--data-dir` chooses
        // the database whatever runs against it, so it opens the app too;
        // the rest are about an operation, and an operation nobody named
        // is a command line to answer rather than a window to open.
        if let Some(stray) = options
            .iter()
            .map(Given::name)
            .find(|name| *name != "--data-dir")
        {
            return Err(Failure::usage(format!(
                "{stray} is about a command, and there is no command here"
            )));
        }
        return Ok(Parsed {
            plan: Plan::Run,
            format,
            data_dir,
        });
    };

    if spec.name == "help" {
        let asked = command_named(&positionals).ok();
        return Ok(Parsed {
            plan: Plan::Help(help::for_command(asked)),
            format,
            data_dir,
        });
    }
    if spec.name == "desktop" {
        return Ok(Parsed {
            plan: desktop(options, positionals)?,
            format,
            data_dir,
        });
    }

    let input = input_source(options)?;
    let mut fields = Map::new();
    fields.insert("op".to_owned(), Value::String(spec.op.to_owned()));
    let extras = request::fields(spec, options, &positionals, input.is_some(), &mut fields)?;

    Ok(Parsed {
        plan: Plan::Operate(Operation {
            request: Value::Object(fields),
            view: spec.view,
            then: spec.then,
            body: extras.body,
            input,
        }),
        format,
        data_dir,
    })
}

/// `--json` and `--format`, which must agree.
fn format_of(options: &[Given]) -> Result<Format, Failure> {
    let named = match single(options, "--format")?.as_deref() {
        None => None,
        Some("text") => Some(Format::Text),
        Some("json") => Some(Format::Json),
        Some("toon") => {
            return Err(Failure::usage(
                "toon is not a format this program writes; the formats are text and json"
                    .to_owned(),
            ));
        }
        Some(other) => {
            return Err(Failure::usage(format!(
                "{other} is not a format; the formats are text and json"
            )));
        }
    };
    let flagged = present(options, "--json").then_some(Format::Json);
    match (named, flagged) {
        (Some(named), Some(flagged)) if named != flagged => Err(Failure::usage(
            "--json and --format text ask for different formats".to_owned(),
        )),
        (Some(format), _) | (_, Some(format)) => Ok(format),
        (None, None) => Ok(Format::Text),
    }
}

/// `--input FILE`, or `-` for standard input.
fn input_source(options: &[Given]) -> Result<Option<Source>, Failure> {
    Ok(single(options, "--input")?.map(|value| {
        if value == "-" {
            Source::Stdin
        } else {
            Source::File(PathBuf::from(value))
        }
    }))
}

/// One occurrence of an option that takes a value. The same option twice
/// with the same value is nobody contradicting themselves; twice with two
/// values is, and is refused rather than silently resolved.
pub fn single(options: &[Given], name: &str) -> Result<Option<String>, Failure> {
    let mut found: Option<&str> = None;
    for given in options.iter().filter(|given| given.name == name) {
        let value = given.value.as_deref().unwrap_or_default();
        match found {
            Some(first) if first != value => {
                return Err(Failure::usage(format!(
                    "{name} is given twice, as {first} and as {value}"
                )));
            }
            _ => found = Some(value),
        }
    }
    Ok(found.map(str::to_owned))
}

pub fn present(options: &[Given], name: &str) -> bool {
    options.iter().any(|given| given.name == name)
}

/// The flags of `jobsdone desktop`, kept exactly as they were.
fn desktop(options: &[Given], positionals: Vec<String>) -> Result<Plan, Failure> {
    if let Some(first) = positionals.first() {
        return Err(Failure::usage(format!(
            "{first} is not an argument of the desktop command"
        )));
    }
    let floats = present(options, "--floating");
    let tiles = present(options, "--tiled");
    if floats && tiles {
        return Err(Failure::usage(
            "--floating and --tiled ask for different windows".to_owned(),
        ));
    }
    let size = match single(options, "--size")? {
        None => None,
        Some(text) => Some(pixels(&text).ok_or_else(|| {
            Failure::usage(format!("{text} is not a size, as in --size 870x650"))
        })?),
    };
    Ok(Plan::Desktop {
        floating: floats.then_some(true).or(tiles.then_some(false)),
        size,
    })
}

/// `870x650` in logical pixels. A number outside what a window may be is
/// held to the range by `WindowSize::new`, which is where that range is.
fn pixels(text: &str) -> Option<(i64, i64)> {
    let (width, height) = text.split_once('x')?;
    Some((width.trim().parse().ok()?, height.trim().parse().ok()?))
}

/// The options as the request builder sees them.
pub type Options = [Given];

/// Every command the program takes, for the help text, in the order it is
/// listed there.
pub fn all_commands() -> &'static [Spec] {
    commands()
}
