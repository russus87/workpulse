//! Trend di produttivita' e confronti temporali.
//!
//! Lavora sui sample (gia' filtrati per intervallo dallo `Store`) e li raggruppa
//! per **giorno locale**, cosi' i confronti riflettono il fuso dell'utente.
//! Tutto puro e testabile: nessun accesso al DB qui dentro.

use crate::model::ActivitySample;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Totali di una giornata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DayTotal {
    /// Giorno locale (YYYY-MM-DD).
    pub day: String,
    /// Secondi attivi (non idle).
    pub active_seconds: i64,
    /// Secondi in attivita' di focus (coding/documenti).
    pub focus_seconds: i64,
}

/// Serie giornaliera ordinata per data, dai sample dati.
pub fn daily(samples: &[ActivitySample]) -> Vec<DayTotal> {
    // (active, focus) per giorno locale.
    let mut by_day: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    for s in samples.iter().filter(|s| !s.idle) {
        let day = local_day(s.start);
        let e = by_day.entry(day).or_insert((0, 0));
        e.0 += s.seconds;
        if s.category.is_focus() {
            e.1 += s.seconds;
        }
    }
    by_day
        .into_iter()
        .map(|(day, (active, focus))| DayTotal {
            day,
            active_seconds: active,
            focus_seconds: focus,
        })
        .collect()
}

/// Confronto tra due valori: corrente vs precedente, con delta percentuale.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comparison {
    pub current: i64,
    pub previous: i64,
    /// Variazione percentuale arrotondata; `None` se il precedente e' 0.
    pub delta_pct: Option<i64>,
}

impl Comparison {
    pub fn new(current: i64, previous: i64) -> Self {
        let delta_pct = if previous == 0 {
            None
        } else {
            Some(((current - previous) as f64 / previous as f64 * 100.0).round() as i64)
        };
        Comparison {
            current,
            previous,
            delta_pct,
        }
    }
}

/// Giorno locale (YYYY-MM-DD) di un istante UTC.
fn local_day(at: DateTime<chrono::Utc>) -> String {
    at.with_timezone(&Local).format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Category;
    use chrono::Utc;

    fn s(cat: Category, secs: i64, idle: bool) -> ActivitySample {
        ActivitySample {
            id: None,
            app: "Code".into(),
            title: String::new(),
            url: None,
            category: cat,
            project: None,
            ticket: None,
            client: None,
            git_branch: None,
            start: Utc::now(),
            seconds: secs,
            idle,
        }
    }

    #[test]
    fn serie_giornaliera_esclude_idle() {
        let samples = vec![
            s(Category::Coding, 600, false),
            s(Category::Communication, 300, false),
            s(Category::Coding, 999, true), // idle: ignorato
        ];
        let d = daily(&samples);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].active_seconds, 900);
        assert_eq!(d[0].focus_seconds, 600);
    }

    #[test]
    fn confronto_percentuale() {
        assert_eq!(Comparison::new(150, 100).delta_pct, Some(50));
        assert_eq!(Comparison::new(50, 100).delta_pct, Some(-50));
        assert_eq!(Comparison::new(10, 0).delta_pct, None);
    }
}
