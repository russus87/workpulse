//! Work Journal: dai sample e dai commit di una giornata costruisce voci
//! per progetto (tempo investito, ticket toccati, commit effettuati).

use crate::model::{ActivitySample, GitCommit, JournalEntry};
use std::collections::BTreeMap;

/// Costruisce le voci di journal per il giorno `day` (YYYY-MM-DD), una per
/// progetto, ordinate dal piu' tempo speso al meno.
pub fn build(day: &str, samples: &[ActivitySample], commits: &[GitCommit]) -> Vec<JournalEntry> {
    // Accumula per progetto: secondi, ticket (set), commit.
    struct Acc {
        seconds: i64,
        tickets: BTreeMap<String, ()>,
        commits: Vec<String>,
    }
    let mut by_project: BTreeMap<String, Acc> = BTreeMap::new();
    let key = |p: &Option<String>| p.clone().unwrap_or_else(|| "(non assegnato)".into());

    for s in samples.iter().filter(|s| !s.idle) {
        let e = by_project.entry(key(&s.project)).or_insert_with(|| Acc {
            seconds: 0,
            tickets: BTreeMap::new(),
            commits: Vec::new(),
        });
        e.seconds += s.seconds;
        if let Some(t) = &s.ticket {
            e.tickets.insert(t.clone(), ());
        }
    }

    for c in commits {
        let e = by_project.entry(key(&c.project)).or_insert_with(|| Acc {
            seconds: 0,
            tickets: BTreeMap::new(),
            commits: Vec::new(),
        });
        e.commits.push(c.message.clone());
    }

    let mut entries: Vec<JournalEntry> = by_project
        .into_iter()
        .map(|(project, a)| JournalEntry {
            day: day.to_string(),
            project: if project == "(non assegnato)" {
                None
            } else {
                Some(project)
            },
            seconds: a.seconds,
            tickets: a.tickets.into_keys().collect(),
            commits: a.commits,
        })
        .collect();

    entries.sort_by(|a, b| b.seconds.cmp(&a.seconds));
    entries
}
