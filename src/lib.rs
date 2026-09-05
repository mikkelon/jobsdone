//! jobsdone: a keyboard-first daily task manager for the terminal.

pub mod app;
pub mod domain;
pub mod input;
pub mod storage;
pub mod terminal;
pub mod ui;

/// The mechanical half of ARCHITECTURE.md: `boundaries` reads the table in
/// section 2 and checks it against what the source actually names,
/// `seam_rules` checks the four rules in section 5 that a scanner can
/// decide.
///
/// Both work from files rather than from the compiler, and both live here
/// rather than in a `tests.rs` because a top-level module of their own
/// would be a module the table does not list.
#[cfg(test)]
mod seams {
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::path::{Path, PathBuf};

    const ARCHITECTURE: &str = "docs/ARCHITECTURE.md";
    const TABLE_HEADING: &str = "## 2. Allowed dependencies";

    /// Path roots that are never a dependency on anything.
    const NEVER_COUNTED: &[&str] = &["std", "core", "alloc", "self", "Self", "super"];

    fn manifest_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    // ---- the table in section 2 -------------------------------------

    /// What one module may name.
    #[derive(Debug, Default)]
    struct Allowed {
        internal: BTreeSet<String>,
        crates: BTreeSet<String>,
    }

    /// Crate names are compared with `-` normalised to `_`, the way they
    /// are written in a path.
    fn normalise(name: &str) -> String {
        name.trim().trim_matches('`').trim().replace('-', "_")
    }

    fn names(cell: &str) -> BTreeSet<String> {
        if cell.trim() == "none" {
            return BTreeSet::new();
        }
        cell.split(',')
            .map(normalise)
            .filter(|name| !name.is_empty())
            .collect()
    }

    fn allowed() -> BTreeMap<String, Allowed> {
        let path = manifest_dir().join(ARCHITECTURE);
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{} could not be read: {error}", path.display()));
        let section = text
            .split_once(TABLE_HEADING)
            .unwrap_or_else(|| panic!("{ARCHITECTURE} has no heading {TABLE_HEADING:?}"))
            .1;

        let mut table = BTreeMap::new();
        for line in section.lines() {
            let line = line.trim();
            if !line.starts_with('|') {
                // Prose before the table, then the end of it.
                if table.is_empty() {
                    continue;
                }
                break;
            }
            let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
            assert_eq!(
                cells.len(),
                3,
                "the table in {TABLE_HEADING} needs three columns, this row has {}: {line}",
                cells.len()
            );

            let module = normalise(cells[0]);
            let is_rule = module.chars().all(|c| c == '-' || c == ':');
            if module.eq_ignore_ascii_case("module") || is_rule {
                continue;
            }
            let previous = table.insert(
                module.clone(),
                Allowed {
                    internal: names(cells[1]),
                    crates: names(cells[2]),
                },
            );
            assert!(previous.is_none(), "{module} appears twice in the table");
        }

        assert!(!table.is_empty(), "no table found under {TABLE_HEADING}");
        table
    }

    // ---- the dependencies in Cargo.toml -----------------------------

    /// The `[dependencies]` and `[dev-dependencies]` keys, normalised.
    fn dependencies() -> (BTreeSet<String>, BTreeSet<String>) {
        let text = fs::read_to_string(manifest_dir().join("Cargo.toml")).expect("Cargo.toml");
        let (mut deps, mut dev) = (BTreeSet::new(), BTreeSet::new());

        let mut section = "";
        for line in text.lines() {
            let line = line.trim();
            if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                // Both `[dependencies]` and `[dependencies.foo]`.
                let (head, sub) = match header.split_once('.') {
                    Some((head, sub)) => (head, Some(sub)),
                    None => (header, None),
                };
                section = match head {
                    "dependencies" => "deps",
                    "dev-dependencies" => "dev",
                    _ => "",
                };
                if let (Some(sub), true) = (sub, !section.is_empty()) {
                    let target = if section == "deps" {
                        &mut deps
                    } else {
                        &mut dev
                    };
                    target.insert(normalise(sub));
                }
                continue;
            }
            if section.is_empty() || line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, _)) = line.split_once('=') {
                let target = if section == "deps" {
                    &mut deps
                } else {
                    &mut dev
                };
                target.insert(normalise(key));
            }
        }
        (deps, dev)
    }

    // ---- the modules on disk ----------------------------------------

    /// Every top-level module and the files it is made of. `src/domain.rs`
    /// and `src/domain/` are one module; `lib.rs` is not a module.
    fn modules() -> BTreeMap<String, Vec<PathBuf>> {
        let src = manifest_dir().join("src");
        let mut found: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();

        for entry in fs::read_dir(&src).expect("src/") {
            let path = entry.expect("a directory entry").path();
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .expect("a usable file name")
                .to_owned();

            if path.is_dir() {
                found.entry(stem).or_default().extend(rust_files(&path));
            } else if path.extension().is_some_and(|e| e == "rs") {
                if stem == "lib" {
                    continue;
                }
                let module = if stem == "main" {
                    "main.rs".to_owned()
                } else {
                    stem
                };
                found.entry(module).or_default().push(path);
            }
        }
        found
    }

    fn rust_files(dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for entry in fs::read_dir(dir).expect("a module directory") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                files.extend(rust_files(&path));
            } else if path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            }
        }
        files
    }

    /// Dev-dependencies are allowed in test files, which cannot reach the
    /// binary (ARCHITECTURE.md section 6 step 5).
    fn is_test_file(path: &Path) -> bool {
        path.file_name().is_some_and(|name| name == "tests.rs")
    }

    // ---- reading a source file --------------------------------------

    /// The source with comments and literals removed, so that a path named
    /// in a doc comment or an error message is not read as a dependency.
    fn strip(source: &str) -> String {
        let chars: Vec<char> = source.chars().collect();
        let mut out = String::with_capacity(source.len());
        let mut i = 0;

        while i < chars.len() {
            let c = chars[i];
            let next = chars.get(i + 1).copied();

            // Comments.
            if c == '/' && next == Some('/') {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            if c == '/' && next == Some('*') {
                let mut depth = 1;
                i += 2;
                while i < chars.len() && depth > 0 {
                    if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                        depth += 1;
                        i += 2;
                    } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                out.push(' ');
                continue;
            }

            // Raw strings: r"...", r#"..."#, br##"..."##.
            if c == 'r' || (c == 'b' && next == Some('r')) {
                let mut j = if c == 'b' { i + 2 } else { i + 1 };
                let hashes = {
                    let start = j;
                    while chars.get(j) == Some(&'#') {
                        j += 1;
                    }
                    j - start
                };
                if chars.get(j) == Some(&'"') {
                    let closing: String = std::iter::once('"')
                        .chain(std::iter::repeat_n('#', hashes))
                        .collect();
                    let rest: String = chars[j + 1..].iter().collect();
                    let end = rest.find(&closing).map_or(chars.len(), |at| {
                        j + 1 + rest[..at].chars().count() + closing.chars().count()
                    });
                    i = end;
                    out.push(' ');
                    continue;
                }
            }

            // Strings.
            if c == '"' {
                i += 1;
                while i < chars.len() {
                    if chars[i] == '\\' {
                        i += 2;
                        continue;
                    }
                    if chars[i] == '"' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                out.push(' ');
                continue;
            }

            // A character literal, but not a lifetime.
            if c == '\'' {
                let escaped = next == Some('\\');
                let simple = chars.get(i + 2) == Some(&'\'');
                if escaped || simple {
                    i += 1;
                    while i < chars.len() {
                        if chars[i] == '\\' {
                            i += 2;
                            continue;
                        }
                        if chars[i] == '\'' {
                            i += 1;
                            break;
                        }
                        i += 1;
                    }
                    out.push(' ');
                    continue;
                }
            }

            out.push(c);
            i += 1;
        }
        out
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Token {
        Ident(String),
        Colons,
        Dot,
        Other,
    }

    fn tokenise(source: &str) -> Vec<Token> {
        let chars: Vec<char> = source.chars().collect();
        let mut tokens = Vec::new();
        let mut i = 0;

        while i < chars.len() {
            let c = chars[i];
            if c.is_alphabetic() || c == '_' {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                tokens.push(Token::Ident(chars[start..i].iter().collect()));
            } else if c == ':' && chars.get(i + 1) == Some(&':') {
                tokens.push(Token::Colons);
                i += 2;
            } else if c == '.' {
                tokens.push(Token::Dot);
                i += 1;
            } else {
                if !c.is_whitespace() {
                    tokens.push(Token::Other);
                }
                i += 1;
            }
        }
        tokens
    }

    /// The roots of every path in a file: `crate::<module>` as an internal
    /// root, and every other `<root>::` as an external one. Bodies count,
    /// not only `use` lines.
    fn roots(source: &str) -> (BTreeSet<String>, BTreeSet<String>) {
        let tokens = tokenise(&strip(source));
        let (mut internal, mut external) = (BTreeSet::new(), BTreeSet::new());

        for (i, token) in tokens.iter().enumerate() {
            let Token::Ident(name) = token else { continue };
            if tokens.get(i + 1) != Some(&Token::Colons) {
                continue;
            }
            // Not a root if something already led here: `a::b::c`, or the
            // turbofish of a method call.
            if i > 0 && matches!(tokens[i - 1], Token::Colons | Token::Dot) {
                continue;
            }
            if NEVER_COUNTED.contains(&name.as_str()) {
                continue;
            }
            if name == "crate" {
                if let Some(Token::Ident(module)) = tokens.get(i + 2) {
                    internal.insert(module.clone());
                }
            } else {
                external.insert(name.clone());
            }
        }
        (internal, external)
    }

    // ---- the tests --------------------------------------------------

    #[test]
    fn boundaries() {
        let table = allowed();
        let (deps, dev_deps) = dependencies();
        let mut broken = Vec::new();

        for (module, files) in modules() {
            let Some(allowed) = table.get(&module) else {
                broken.push(format!(
                    "module `{module}` is not a row in {TABLE_HEADING} of {ARCHITECTURE}"
                ));
                continue;
            };

            for file in files {
                let source = fs::read_to_string(&file).expect("a source file");
                let (internal, external) = roots(&source);
                let where_ = file
                    .strip_prefix(manifest_dir())
                    .unwrap_or(&file)
                    .display()
                    .to_string();

                for root in internal {
                    if root == module || allowed.internal.contains(&root) {
                        continue;
                    }
                    broken.push(format!(
                        "{where_} names crate::{root}, which `{module}` may not"
                    ));
                }

                for root in external {
                    // Only crates count; a local type used as a path root
                    // is not a dependency.
                    if !deps.contains(&root) && !dev_deps.contains(&root) {
                        continue;
                    }
                    if allowed.crates.contains(&root) {
                        continue;
                    }
                    if dev_deps.contains(&root) && is_test_file(&file) {
                        continue;
                    }
                    broken.push(format!("{where_} names {root}, which `{module}` may not"));
                }
            }
        }

        assert!(
            broken.is_empty(),
            "the dependency table in {ARCHITECTURE} forbids these:\n  {}\n\nChange the table \
             first, in the commit that needs it, or move the code.",
            broken.join("\n  ")
        );
    }

    /// Is `needle` in `haystack` as a whole word rather than inside a
    /// longer identifier?
    fn contains_token(haystack: &str, needle: &str) -> bool {
        let ident = |c: char| c.is_alphanumeric() || c == '_';
        let mut from = 0;
        while let Some(at) = haystack[from..].find(needle) {
            let start = from + at;
            let end = start + needle.len();
            let before = haystack[..start].chars().next_back();
            let after = haystack[end..].chars().next();
            let joined = before.is_some_and(ident) || after.is_some_and(ident);
            if !joined {
                return true;
            }
            from = start + 1;
        }
        false
    }

    fn forbid(module: &str, needles: &[&str], rule: &str, broken: &mut Vec<String>) {
        let files = modules().remove(module).unwrap_or_default();
        for file in files {
            let source = strip(&fs::read_to_string(&file).expect("a source file"));
            let where_ = file
                .strip_prefix(manifest_dir())
                .unwrap_or(&file)
                .display()
                .to_string();
            for needle in needles {
                if contains_token(&source, needle) {
                    broken.push(format!("{where_} contains `{needle}`: {rule}"));
                }
            }
        }
    }

    #[test]
    fn seam_rules() {
        let mut broken = Vec::new();

        forbid(
            "domain",
            &["Zoned::now", "SystemTime", "Instant"],
            "rule 2, the domain takes time as an argument",
            &mut broken,
        );
        forbid(
            "ui",
            &["&mut App", "&mut Model"],
            "rule 4, rendering is pure",
            &mut broken,
        );
        forbid(
            "app",
            &["KeyCode"],
            "rule 5, actions carry no ids and app never matches on a key code",
            &mut broken,
        );

        // Rule 10: a failed commit is a hint, never a panic. A test that
        // asserts a commit succeeded is not the application path.
        for (_, files) in modules() {
            for file in files {
                if is_test_file(&file) {
                    continue;
                }
                let source = strip(&fs::read_to_string(&file).expect("a source file"));
                let where_ = file
                    .strip_prefix(manifest_dir())
                    .unwrap_or(&file)
                    .display()
                    .to_string();
                for (number, line) in source.lines().enumerate() {
                    let panics = contains_token(line, "unwrap") || contains_token(line, "expect");
                    if line.contains("commit(") && panics {
                        broken.push(format!(
                            "{where_}:{} unwraps a commit: rule 10, a failed commit is a hint",
                            number + 1
                        ));
                    }
                }
            }
        }

        assert!(
            broken.is_empty(),
            "ARCHITECTURE.md section 5 forbids these:\n  {}",
            broken.join("\n  ")
        );
    }
}
