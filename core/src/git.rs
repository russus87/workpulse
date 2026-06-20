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
    // Ogni commit inizia con 0x1e; i campi sono separati da 0x1f. Dopo la riga
    // dei campi, `--numstat` aggiunge righe "aggiunte\trimosse\tpercorso".
    let fmt = "\x1e%H\x1f%an\x1f%aI\x1f%s";
    let out = Command::new("git")
        .args([
            "-C",
            repo_path,
            "log",
            &format!("--author={author_email}"),
            &format!("--since={since}"),
            "--numstat",
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
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(parse_log(&text, &repo, &branch))
}

/// Parsing puro dell'output di `git log --numstat` (testabile senza un repo).
pub fn parse_log(text: &str, repo: &str, branch: &str) -> Vec<GitCommit> {
    let ticket_re = Regex::new(r"\b([A-Z][A-Z0-9]{1,9})-\d{1,6}\b").unwrap();
    let mut commits = Vec::new();
    for record in text.split('\x1e') {
        let record = record.trim_matches(|c| c == '\n' || c == '\r');
        if record.is_empty() {
            continue;
        }
        let mut lines = record.lines();
        let header = match lines.next() {
            Some(h) => h,
            None => continue,
        };
        let f: Vec<&str> = header.split('\x1f').collect();
        if f.len() < 4 {
            continue;
        }
        // Somma le righe numstat (le righe binarie hanno "-").
        let (mut additions, mut deletions) = (0i64, 0i64);
        for l in lines {
            let cols: Vec<&str> = l.split('\t').collect();
            if cols.len() >= 2 {
                additions += cols[0].parse::<i64>().unwrap_or(0);
                deletions += cols[1].parse::<i64>().unwrap_or(0);
            }
        }
        let at: DateTime<Utc> = DateTime::parse_from_rfc3339(f[2])
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        let project = ticket_re
            .captures(f[3])
            .map(|c| c.get(1).unwrap().as_str().to_string());
        commits.push(GitCommit {
            repo: repo.to_string(),
            hash: f[0].to_string(),
            author: f[1].to_string(),
            message: f[3].to_string(),
            branch: branch.to_string(),
            project,
            at,
            additions,
            deletions,
        });
    }
    commits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_log_numstat() {
        let text = "\x1eabc123\x1fMario\x1f2026-06-20T10:00:00+00:00\x1ffix: PAM-12 bug\n10\t2\tsrc/a.rs\n5\t0\tsrc/b.rs\n\x1edef456\x1fMario\x1f2026-06-20T11:00:00+00:00\x1ffeat: ui\n3\t1\tui.svelte\n";
        let commits = parse_log(text, "pam", "main");
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].hash, "abc123");
        assert_eq!(commits[0].project.as_deref(), Some("PAM"));
        assert_eq!(commits[0].additions, 15);
        assert_eq!(commits[0].deletions, 2);
        assert_eq!(commits[1].additions, 3);
    }
}
