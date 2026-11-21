//! Files in the app config directory: `rules.toml`, `settings.toml`, the append-only
//! `audit.jsonl` (rotated to `audit.jsonl.1` at [`AUDIT_MAX_BYTES`]) and `auto-memo.json` (what
//! `auto` rules did, see `exec::AutoMemo`).
//!
//! Everything here is blocking file I/O; async code runs it through [`blocking`].

use crate::settings::{Settings, default_settings_toml, load_settings, validate_settings};
use prism_rules::{RuleSet, default_rules_toml, load_rules, validate};
use protocol::AuditEntry;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

const RULES_FILE: &str = "rules.toml";
const SETTINGS_FILE: &str = "settings.toml";
const AUDIT_FILE: &str = "audit.jsonl";
/// The previous audit log, replaced on every rotation.
const AUDIT_OLD_FILE: &str = "audit.jsonl.1";
/// Size from which `audit.jsonl` is rotated before the next line is appended.
const AUDIT_MAX_BYTES: u64 = 1024 * 1024;
const MEMO_FILE: &str = "auto-memo.json";

/// Runs blocking work (file I/O) on the blocking thread pool, off the async runtime and the main
/// thread. A panic in `job` continues here.
pub(crate) async fn blocking<T: Send + 'static>(job: impl FnOnce() -> T + Send + 'static) -> T {
    match tauri::async_runtime::spawn_blocking(job).await {
        Ok(value) => value,
        Err(tauri::Error::JoinError(error)) if error.is_panic() => {
            std::panic::resume_unwind(error.into_panic())
        }
        Err(error) => panic!("blocking job did not finish: {error}"),
    }
}

/// Reads `name` in `dir`, creating it (and the directory) with `default` when missing.
fn ensure_file(dir: &Path, name: &str, default: &str) -> io::Result<String> {
    fs::create_dir_all(dir)?;
    match fs::read_to_string(dir.join(name)) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::write(dir.join(name), default)?;
            Ok(default.to_owned())
        }
        Err(error) => Err(error),
    }
}

/// Writes `name` atomically (temporary file, then rename).
fn write_file(dir: &Path, name: &str, text: &str) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let temporary = dir.join(format!("{name}.tmp"));
    fs::write(&temporary, text)?;
    fs::rename(&temporary, dir.join(name))
}

/// Reads `rules.toml`, creating it (and the directory) with the default rules when missing.
pub(crate) fn ensure_rules(dir: &Path) -> io::Result<String> {
    ensure_file(dir, RULES_FILE, default_rules_toml())
}

/// Reads `settings.toml`, creating it with the defaults (an empty scope) when missing.
pub(crate) fn ensure_settings(dir: &Path) -> io::Result<String> {
    ensure_file(dir, SETTINGS_FILE, default_settings_toml())
}

/// Parses settings text; the settings plus warnings (patterns that never match). A text that is
/// not valid TOML is an error.
pub(crate) fn parse_settings(text: &str) -> Result<(Settings, Vec<String>), String> {
    let settings = load_settings(text)?;
    let warnings = validate_settings(&settings);
    Ok((settings, warnings))
}

/// A non-empty `[gh] path` must be an existing executable file; an error text otherwise.
pub(crate) fn check_gh_path(settings: &Settings) -> Result<(), String> {
    let path = settings.gh.path.trim();
    if path.is_empty() {
        return Ok(());
    }
    let metadata = fs::metadata(path)
        .map_err(|error| format!("[gh] path `{path}` cannot be used: {error}"))?;
    if !metadata.is_file() {
        return Err(format!("[gh] path `{path}` is not a file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(format!("[gh] path `{path}` is not executable"));
        }
    }
    Ok(())
}

/// Writes `settings.toml` atomically.
pub(crate) fn write_settings(dir: &Path, text: &str) -> io::Result<()> {
    write_file(dir, SETTINGS_FILE, text)
}

/// Parses rules text; the rule set plus warnings (rules without actions, bad regexes). A text
/// that is not valid TOML or has unknown actions is an error.
pub(crate) fn parse_rules(text: &str) -> Result<(RuleSet, Vec<String>), String> {
    let set = load_rules(text)?;
    let warnings = validate(&set.rules);
    Ok((set, warnings))
}

/// Writes `rules.toml` atomically (temporary file, then rename).
pub(crate) fn write_rules(dir: &Path, text: &str) -> io::Result<()> {
    write_file(dir, RULES_FILE, text)
}

/// Appends one line to `audit.jsonl`, first rotating it when it reached [`AUDIT_MAX_BYTES`].
pub(crate) fn append_audit(dir: &Path, entry: &AuditEntry) -> io::Result<()> {
    append_audit_rotating(dir, entry, AUDIT_MAX_BYTES)
}

fn append_audit_rotating(dir: &Path, entry: &AuditEntry, max_bytes: u64) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let path = dir.join(AUDIT_FILE);
    if fs::metadata(&path).is_ok_and(|metadata| metadata.len() >= max_bytes) {
        // A failed rotation must not lose the line: it is appended to the full file then.
        let _ = fs::rename(&path, dir.join(AUDIT_OLD_FILE));
    }
    let mut line = serde_json::to_string(entry).map_err(io::Error::other)?;
    line.push('\n');
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(line.as_bytes())
}

/// The newest `limit` entries (of `audit.jsonl`, then of the rotated `audit.jsonl.1`), newest
/// first. Unreadable lines are skipped.
pub(crate) fn read_audit(dir: &Path, limit: usize) -> Vec<AuditEntry> {
    let mut entries = Vec::new();
    for name in [AUDIT_FILE, AUDIT_OLD_FILE] {
        let wanted = limit.saturating_sub(entries.len());
        if wanted == 0 {
            break;
        }
        let Ok(text) = fs::read_to_string(dir.join(name)) else {
            continue;
        };
        entries.extend(
            text.lines()
                .rev()
                .filter_map(|line| serde_json::from_str(line).ok())
                .take(wanted),
        );
    }
    entries
}

/// The keys of the `auto` actions done so far; empty when the file is missing or unreadable.
pub(crate) fn read_memo(dir: &Path) -> Vec<String> {
    fs::read_to_string(dir.join(MEMO_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Writes the keys of the `auto` actions done so far (a JSON array of strings) atomically.
pub(crate) fn write_memo(dir: &Path, json: &str) -> io::Result<()> {
    write_file(dir, MEMO_FILE, json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::{Action, ActionResult, PrRef};

    /// A temporary directory with a not yet existing config directory inside.
    fn temp_dir() -> (tempfile::TempDir, std::path::PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("config");
        (root, dir)
    }

    fn entry(number: u64) -> AuditEntry {
        AuditEntry {
            at: "2026-11-21T10:00:00Z".to_owned(),
            rule: None,
            result: ActionResult {
                pr: PrRef {
                    repo: "acme/widget".to_owned(),
                    number,
                },
                action: Action::Approve,
                ok: true,
                message: "approved".to_owned(),
            },
        }
    }

    #[test]
    fn rules_file_is_created_and_replaced() {
        let (_root, dir) = temp_dir();
        let text = ensure_rules(&dir).unwrap();
        assert_eq!(text, default_rules_toml());
        write_rules(&dir, "[[rule]]\nname = \"x\"\n").unwrap();
        assert_eq!(ensure_rules(&dir).unwrap(), "[[rule]]\nname = \"x\"\n");
        assert!(!dir.join("rules.toml.tmp").exists());
    }

    #[test]
    fn parse_rules_errors_and_warnings() {
        assert!(parse_rules("[[rule]]\nactions = [\"explode\"]").is_err());
        let (set, warnings) = parse_rules("[[rule]]\nname = \"x\"\n").unwrap();
        assert_eq!(set.rules.len(), 1);
        assert_eq!(warnings.len(), 1);
        let (_, none) = parse_rules(default_rules_toml()).unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn settings_file_is_created_parsed_and_replaced() {
        let (_root, dir) = temp_dir();
        let text = ensure_settings(&dir).unwrap();
        assert_eq!(text, default_settings_toml());
        let (settings, warnings) = parse_settings(&text).unwrap();
        assert!(settings.scope.is_empty() && warnings.is_empty());
        let edited = "[scope]\ninclude = [\"acme/*\", \"x\"]\n";
        write_settings(&dir, edited).unwrap();
        assert_eq!(ensure_settings(&dir).unwrap(), edited);
        let (settings, warnings) = parse_settings(edited).unwrap();
        assert_eq!(settings.scope.include, ["acme/*", "x"]);
        assert_eq!(warnings.len(), 1);
        assert!(parse_settings("[scope\n").is_err());
        assert!(!dir.join("settings.toml.tmp").exists());
    }

    #[test]
    fn gh_path_must_be_an_executable_file() {
        let (_root, dir) = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let with = |path: &str| {
            let mut settings = Settings::default();
            settings.gh.path = path.to_owned();
            check_gh_path(&settings)
        };
        assert!(with("").is_ok() && with("  ").is_ok());
        let missing = dir.join("nope");
        assert!(with(missing.to_str().unwrap()).is_err());
        assert!(
            with(dir.to_str().unwrap())
                .unwrap_err()
                .contains("not a file")
        );
        let file = dir.join("gh");
        fs::write(&file, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
            assert!(
                with(file.to_str().unwrap())
                    .unwrap_err()
                    .contains("not executable")
            );
            fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert!(with(file.to_str().unwrap()).is_ok());
    }

    #[test]
    fn memo_roundtrip() {
        let (_root, dir) = temp_dir();
        assert!(read_memo(&dir).is_empty());
        let keys = vec!["a/b#1|abc|approve".to_owned()];
        fs::create_dir_all(&dir).unwrap();
        write_memo(&dir, &serde_json::to_string(&keys).unwrap()).unwrap();
        assert_eq!(read_memo(&dir), keys);
        fs::write(dir.join(MEMO_FILE), "garbage").unwrap();
        assert!(read_memo(&dir).is_empty());
    }

    #[test]
    fn audit_is_appended_and_read_newest_first() {
        let (_root, dir) = temp_dir();
        assert!(read_audit(&dir, 10).is_empty());
        for number in 1..=3 {
            append_audit(&dir, &entry(number)).unwrap();
        }
        let numbers: Vec<u64> = read_audit(&dir, 2)
            .iter()
            .map(|e| e.result.pr.number)
            .collect();
        assert_eq!(numbers, [3, 2]);
    }

    #[test]
    fn audit_is_rotated_and_read_across_both_files() {
        let (_root, dir) = temp_dir();
        let line_len = serde_json::to_string(&entry(1)).unwrap().len() as u64 + 1;
        for number in 1..=5 {
            append_audit_rotating(&dir, &entry(number), 2 * line_len).unwrap();
        }
        // 1 2 | 3 4 rotated out, then 5 into a new file: the oldest lines are gone.
        let numbers = |limit| -> Vec<u64> {
            read_audit(&dir, limit)
                .iter()
                .map(|e| e.result.pr.number)
                .collect()
        };
        assert_eq!(numbers(10), [5, 4, 3]);
        assert_eq!(numbers(2), [5, 4]);
        assert_eq!(numbers(0), Vec::<u64>::new());
    }

    #[test]
    fn blocking_runs_the_job() {
        let answer = tauri::async_runtime::block_on(blocking(|| 6 * 7));
        assert_eq!(answer, 42);
    }
}
