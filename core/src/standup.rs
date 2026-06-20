//! Generatore di "standup": un recap copiabile (Markdown) di cosa hai fatto,
//! pensato per il daily. Riusa sample, commit e meeting gia' tracciati.

use crate::aggregate::human_duration;
use crate::model::{ActivitySample, GitCommit, Meeting};
use std::collections::BTreeMap;

/// Costruisce il testo dello standup per un intervallo (es. "ieri" o "oggi").
/// `label` e' l'intestazione (es. "Ieri", "Oggi").
pub fn standup(
    label: &str,
    samples: &[ActivitySample],
    commits: &[GitCommit],
    meetings: &[Meeting],
) -> String {
    let mut out = format!("**{label}**\n");

    // Tempo per progetto.
    let mut by_project: BTreeMap<String, i64> = BTreeMap::new();
    for s in samples.iter().filter(|s| !s.idle) {
        if let Some(p) = &s.project {
            *by_project.entry(p.clone()).or_default() += s.seconds;
        }
    }
    let mut projects: Vec<(String, i64)> = by_project.into_iter().collect();
    projects.sort_by(|a, b| b.1.cmp(&a.1));
    for (p, secs) in projects {
        out.push_str(&format!("- {p}: {}\n", human_duration(secs)));
        // Commit del progetto.
        for c in commits.iter().filter(|c| c.project.as_deref() == Some(p.as_str())) {
            out.push_str(&format!("  - {} ({}/+{} -{})\n", c.message, short(&c.hash), c.additions, c.deletions));
        }
    }

    if !meetings.is_empty() {
        out.push_str(&format!("- Meeting: {}\n", meetings.len()));
        for m in meetings {
            out.push_str(&format!("  - {} ({})\n", m.subject, human_duration(m.duration_seconds)));
        }
    }

    if projects_empty(samples) && commits.is_empty() && meetings.is_empty() {
        out.push_str("- (nessuna attivita' registrata)\n");
    }
    out
}

fn projects_empty(samples: &[ActivitySample]) -> bool {
    !samples.iter().any(|s| !s.idle && s.project.is_some())
}

fn short(hash: &str) -> &str {
    &hash[..hash.len().min(7)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Category;
    use chrono::Utc;

    #[test]
    fn standup_con_progetto_e_commit() {
        let samples = vec![ActivitySample {
            id: None,
            app: "Code".into(),
            title: String::new(),
            url: None,
            category: Category::Coding,
            project: Some("PAM".into()),
            ticket: None,
            client: None,
            git_branch: None,
            start: Utc::now(),
            seconds: 3600,
            idle: false,
        }];
        let commits = vec![GitCommit {
            repo: "pam".into(),
            hash: "abcdef1234".into(),
            author: "me".into(),
            message: "fix: login".into(),
            branch: "main".into(),
            project: Some("PAM".into()),
            at: Utc::now(),
            additions: 10,
            deletions: 3,
        }];
        let out = standup("Oggi", &samples, &commits, &[]);
        assert!(out.contains("**Oggi**"));
        assert!(out.contains("PAM: 1h 0m"));
        assert!(out.contains("fix: login"));
        assert!(out.contains("abcdef1"));
    }
}
