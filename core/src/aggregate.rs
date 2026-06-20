//! Aggregazione e metriche di produttivita' a partire dai sample.
//!
//! Tutte le funzioni lavorano su una lista di `ActivitySample` gia' ordinata
//! per `start` (come la restituisce `Store::samples_between`). Sono pure e
//! testabili: nessun accesso al DB qui dentro.

use crate::model::{ActivitySample, ProductivityMetrics};

/// Soglia (secondi) sotto la quale due attivita' troppo ravvicinate su contesti
/// diversi contano come "interruzione" piu' che come lavoro pianificato.
const INTERRUPTION_MAX_SECONDS: i64 = 120;

/// Calcola le metriche di produttivita' su una sequenza di sample.
///
/// - `active_seconds`: somma dei secondi non idle.
/// - `focus_seconds`: secondi in categorie di focus (coding/documenti) non idle.
/// - `context_switches`: quante volte cambia il "contesto" (progetto se presente,
///   altrimenti app) tra un sample non idle e il successivo.
/// - `interruptions`: cambi verso una categoria di comunicazione che spezzano
///   un blocco di focus e durano poco (rientro rapido = vera interruzione).
/// - `avg_focus_block_seconds`: durata media dei blocchi di focus contigui.
pub fn metrics(samples: &[ActivitySample]) -> ProductivityMetrics {
    let active: Vec<&ActivitySample> = samples.iter().filter(|s| !s.idle).collect();

    let active_seconds = active.iter().map(|s| s.seconds).sum();
    let focus_seconds = active
        .iter()
        .filter(|s| s.category.is_focus())
        .map(|s| s.seconds)
        .sum();

    let ctx = |s: &ActivitySample| s.project.clone().unwrap_or_else(|| s.app.clone());

    let mut context_switches = 0;
    let mut interruptions = 0;
    for w in active.windows(2) {
        if ctx(w[0]) != ctx(w[1]) {
            context_switches += 1;
            let broke_focus = w[0].category.is_focus() && !w[1].category.is_focus();
            if broke_focus
                && matches!(w[1].category, crate::model::Category::Communication)
                && w[1].seconds <= INTERRUPTION_MAX_SECONDS
            {
                interruptions += 1;
            }
        }
    }

    // Blocchi di focus contigui (stesso contesto, categoria di focus).
    let mut blocks: Vec<i64> = Vec::new();
    let mut current: i64 = 0;
    let mut prev_ctx: Option<String> = None;
    for s in &active {
        if s.category.is_focus() {
            let c = ctx(s);
            if prev_ctx.as_deref() == Some(c.as_str()) {
                current += s.seconds;
            } else {
                if current > 0 {
                    blocks.push(current);
                }
                current = s.seconds;
                prev_ctx = Some(c);
            }
        } else {
            if current > 0 {
                blocks.push(current);
            }
            current = 0;
            prev_ctx = None;
        }
    }
    if current > 0 {
        blocks.push(current);
    }
    let avg_focus_block_seconds = if blocks.is_empty() {
        0
    } else {
        blocks.iter().sum::<i64>() / blocks.len() as i64
    };

    ProductivityMetrics {
        active_seconds,
        focus_seconds,
        context_switches,
        interruptions,
        avg_focus_block_seconds,
    }
}

/// Formatta una durata in secondi come "3h 20m" / "45m" / "30s".
pub fn human_duration(seconds: i64) -> String {
    let h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    let s = seconds % 60;
    if h > 0 {
        format!("{h}h {m}m")
    } else if m > 0 {
        format!("{m}m")
    } else {
        format!("{s}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ActivitySample, Category};
    use chrono::Utc;

    fn sample(app: &str, project: Option<&str>, cat: Category, secs: i64) -> ActivitySample {
        ActivitySample {
            id: None,
            app: app.into(),
            title: String::new(),
            url: None,
            category: cat,
            project: project.map(|p| p.into()),
            ticket: None,
            client: None,
            git_branch: None,
            start: Utc::now(),
            seconds: secs,
            idle: false,
        }
    }

    #[test]
    fn focus_switch_e_interruzioni() {
        let s = vec![
            sample("Code", Some("PAM"), Category::Coding, 1800),
            sample("Teams", None, Category::Communication, 60), // interruzione
            sample("Code", Some("PAM"), Category::Coding, 1200),
        ];
        let m = metrics(&s);
        assert_eq!(m.active_seconds, 3060);
        assert_eq!(m.focus_seconds, 3000);
        assert_eq!(m.context_switches, 2);
        assert_eq!(m.interruptions, 1);
    }

    #[test]
    fn durata_leggibile() {
        assert_eq!(human_duration(3 * 3600 + 20 * 60), "3h 20m");
        assert_eq!(human_duration(45 * 60), "45m");
    }
}
