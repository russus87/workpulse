//! Heatmap di produttivita': secondi attivi per (giorno della settimana, ora),
//! in fuso locale. Utile per visualizzare quando lavori davvero.

use crate::model::ActivitySample;
use chrono::{Datelike, Local, Timelike};
use serde::Serialize;

/// Una cella della heatmap.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HeatCell {
    /// 0 = lunedi' ... 6 = domenica.
    pub weekday: u32,
    /// 0..23 ora locale.
    pub hour: u32,
    pub seconds: i64,
}

/// Costruisce la matrice 7x24 (appiattita in celle non vuote).
pub fn heatmap(samples: &[ActivitySample]) -> Vec<HeatCell> {
    let mut grid = [[0i64; 24]; 7];
    for s in samples.iter().filter(|s| !s.idle) {
        let local = s.start.with_timezone(&Local);
        let wd = local.weekday().num_days_from_monday() as usize;
        let h = local.hour() as usize;
        grid[wd][h] += s.seconds;
    }
    let mut out = Vec::new();
    for (wd, hours) in grid.iter().enumerate() {
        for (h, &secs) in hours.iter().enumerate() {
            if secs > 0 {
                out.push(HeatCell {
                    weekday: wd as u32,
                    hour: h as u32,
                    seconds: secs,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Category;
    use chrono::Utc;

    #[test]
    fn celle_non_vuote() {
        let s = ActivitySample {
            id: None,
            app: "Code".into(),
            title: String::new(),
            url: None,
            category: Category::Coding,
            project: None,
            ticket: None,
            client: None,
            git_branch: None,
            start: Utc::now(),
            seconds: 600,
            idle: false,
        };
        let cells = heatmap(&[s]);
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].seconds, 600);
    }
}
