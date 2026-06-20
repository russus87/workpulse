//! Integrazione Git "senza dipendenze": invochiamo il comando `git` gia'
//! presente sul sistema dello sviluppatore. Evita di linkare libgit2 e mantiene
//! il core leggero da pacchettizzare.
//!
//! Leggiamo solo informazioni (log, branch, nome repo): nessuna scrittura.

use crate::error::{Error, Result};
use crate::model::GitCommit;
use chrono::{DateTime, Utc};
use regex::Regex;
use std::process::Command;

/// Branch attualmente in checkout in `repo_path`, se determinabile.
pub fn current_branch(repo_path: &str) -> Option<String> {
    let out = Command::new("git")
        .args(["-C", repo_path, "rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let b = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if b.is_empty() || b == "HEAD" {
        None
    } else {
        Some(b)
    }
}

/// Nome "corto" del repo (ultima componente del path o della remote URL).
fn repo_name(repo_path: &str) -> String {
    repo_path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(repo_path)
        .to_string()
}

/// Legge i commit di `repo_path` effettuati dall'autore `author_email`
/// a partire da `since` (formato accettato da `git log --since`, es. "1 day ago").
///
/// `project` viene dedotto dal primo ticket trovato nel messaggio (regex `ABC-123`).
pub fn read_commits(
    repo_path: &str,
    author_email: &str,
    since: &str,
) -> Result<Vec<GitCommit>> {
    // Formato a campi separati da carattere di unita' (0x1f) e record da 0x1e.
    let fmt = "%H\x1f%an\x1f%aI\x1f%s\x1e";
    let out = Command::new("git")
        .args([
            "-C",
            repo_path,
            "log",
            &format!("--author={author_email}"),
            &format!("--since={since}"),
            &format!("--pretty=format:{fmt}"),
        ])
        .output()
        .map_err(|e| Error::Other(format!("git log fallito: {e}")))?;

    if !out.status.success() {
        return Err(Error::Other(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }

    let branch = current_branch(repo_path).unwrap_or_else(|| "HEAD".into());
    let repo = repo_name(repo_path);
    let ticket_re = Regex::new(r"\b([A-Z][A-Z0-9]{1,9})-\d{1,6}\b").unwrap();
    let text = String::from_utf8_lossy(&out.stdout);

    let mut commits = Vec::new();
    for record in text.split('\x1e') {
        let record = record.trim_matches(|c| c == '\n' || c == '\r');
        if record.is_empty() {
            continue;
        }
        let f: Vec<&str> = record.split('\x1f').collect();
        if f.len() < 4 {
            continue;
        }
        let at: DateTime<Utc> = DateTime::parse_from_rfc3339(f[2])
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        let project = ticket_re
            .captures(f[3])
            .map(|c| c.get(1).unwrap().as_str().to_string());
        commits.push(GitCommit {
            repo: repo.clone(),
            hash: f[0].to_string(),
            author: f[1].to_string(),
            message: f[3].to_string(),
            branch: branch.clone(),
            project,
            at,
        });
    }
    Ok(commits)
}
