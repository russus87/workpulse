//! Statistiche di sviluppo: ripartizione del tempo per linguaggio (dedotto dal
//! nome file nel titolo dell'editor) e totali di codice dai commit.

use crate::model::{ActivitySample, Category, GitCommit, UsageRow};
use regex::Regex;
use std::collections::HashMap;

/// Mappa estensione -> nome linguaggio leggibile.
fn language_of(ext: &str) -> Option<&'static str> {
    Some(match ext.to_lowercase().as_str() {
        "rs" => "Rust",
        "ts" | "tsx" => "TypeScript",
        "js" | "jsx" | "mjs" => "JavaScript",
        "svelte" => "Svelte",
        "py" => "Python",
        "go" => "Go",
        "java" => "Java",
        "kt" => "Kotlin",
        "c" | "h" => "C",
        "cpp" | "cc" | "hpp" => "C++",
        "cs" => "C#",
        "rb" => "Ruby",
        "php" => "PHP",
        "sql" => "SQL",
        "sh" | "bash" | "fish" => "Shell",
        "html" => "HTML",
        "css" | "scss" => "CSS",
        "json" | "yaml" | "yml" | "toml" => "Config",
        "md" => "Markdown",
        _ => return None,
    })
}

/// Tempo per linguaggio, dai sample di coding (estensione nel titolo finestra).
pub fn by_language(samples: &[ActivitySample]) -> Vec<UsageRow> {
    // Cattura "qualcosa.ext" nel titolo (es. "main.rs - workpulse - VSCode").
    let re = Regex::new(r"[\w\-]+\.([A-Za-z0-9]{1,6})\b").unwrap();
    let mut by: HashMap<String, i64> = HashMap::new();
    for s in samples
        .iter()
        .filter(|s| !s.idle && matches!(s.category, Category::Coding))
    {
        if let Some(c) = re.captures(&s.title) {
            if let Some(lang) = language_of(c.get(1).unwrap().as_str()) {
                *by.entry(lang.to_string()).or_default() += s.seconds;
            }
        }
    }
    let mut rows: Vec<UsageRow> = by
        .into_iter()
        .map(|(key, seconds)| UsageRow { key, seconds })
        .collect();
    rows.sort_by(|a, b| b.seconds.cmp(&a.seconds));
    rows
}

/// Totali di codice (commit, righe aggiunte/rimosse) su un insieme di commit.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CodeTotals {
    pub commits: usize,
    pub additions: i64,
    pub deletions: i64,
}

pub fn code_totals(commits: &[GitCommit]) -> CodeTotals {
    CodeTotals {
        commits: commits.len(),
        additions: commits.iter().map(|c| c.additions).sum(),
        deletions: commits.iter().map(|c| c.deletions).sum(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn coding(title: &str, secs: i64) -> ActivitySample {
        ActivitySample {
            id: None,
            app: "Code".into(),
            title: title.into(),
            url: None,
            category: Category::Coding,
            project: None,
            ticket: None,
            client: None,
            git_branch: None,
            start: Utc::now(),
            seconds: secs,
            idle: false,
        }
    }

    #[test]
    fn ripartizione_per_linguaggio() {
        let samples = vec![
            coding("main.rs - workpulse", 600),
            coding("App.svelte - ui", 300),
            coding("lib.rs - core", 200),
        ];
        let rows = by_language(&samples);
        assert_eq!(rows[0].key, "Rust");
        assert_eq!(rows[0].seconds, 800);
        assert!(rows.iter().any(|r| r.key == "Svelte" && r.seconds == 300));
    }
}
