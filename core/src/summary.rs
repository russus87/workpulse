//! Generazione del riepilogo in linguaggio naturale (stile "AI Summary").
//!
//! Di default WorkPulse produce il testo con un modello a template *locale*:
//! deterministico, istantaneo, privacy-safe (nessun dato lascia il dispositivo).
//! L'integrazione con un LLM (per riformulazioni piu' ricche) e' un'opzione
//! esplicita e disattivata di default — vedi `SummaryInput` come contratto di
//! ingresso che un generatore LLM potrebbe ricevere al posto del template.

use crate::aggregate::{human_duration, metrics};
use crate::model::{ActivitySample, Category, GitCommit, Meeting};
use regex::Regex;

/// Dati sintetici di una giornata: l'input sia per il template sia (in futuro)
/// per un generatore LLM. Tenerlo serializzabile mantiene i due intercambiabili.
pub struct SummaryInput<'a> {
    pub samples: &'a [ActivitySample],
    pub commits: &'a [GitCommit],
    /// Meeting reali (da calendario). Se vuoto, il conteggio ricade sull'euristica.
    pub meetings: &'a [Meeting],
}

/// Progetto su cui si e' speso piu' tempo, con i relativi secondi.
fn top_project(samples: &[ActivitySample]) -> Option<(String, i64)> {
    use std::collections::HashMap;
    let mut by: HashMap<String, i64> = HashMap::new();
    for s in samples.iter().filter(|s| !s.idle) {
        if let Some(p) = &s.project {
            *by.entry(p.clone()).or_default() += s.seconds;
        }
    }
    by.into_iter().max_by_key(|(_, s)| *s)
}

/// Conta i commit che sembrano correzioni di bug (fix/bug/hotfix/risolto).
fn count_bugfix(commits: &[GitCommit]) -> usize {
    let re = Regex::new(r"(?i)\b(fix|bug|hotfix|risolt|patch)").unwrap();
    commits.iter().filter(|c| re.is_match(&c.message)).count()
}

/// Numero di meeting. Se sono disponibili meeting reali dal calendario li usa;
/// altrimenti ricade sull'euristica (blocchi di comunicazione >= 10 minuti).
fn count_meetings(samples: &[ActivitySample], meetings: &[Meeting]) -> usize {
    if !meetings.is_empty() {
        return meetings.len();
    }
    samples
        .iter()
        .filter(|s| matches!(s.category, Category::Communication) && s.seconds >= 600)
        .count()
}

/// Genera il riepilogo testuale della giornata.
///
/// Esempio prodotto:
/// "Oggi hai lavorato 3h 10m sul progetto PAM, corretto 4 bug,
///  effettuato 6 commit e partecipato a 2 meeting. Focus: 2h 40m
///  (5 cambi di contesto, 2 interruzioni)."
pub fn daily_summary(input: &SummaryInput) -> String {
    let m = metrics(input.samples);
    if m.active_seconds == 0 && input.commits.is_empty() {
        return "Oggi non risultano attivita' tracciate.".to_string();
    }

    let mut parts: Vec<String> = Vec::new();

    match top_project(input.samples) {
        Some((p, secs)) => parts.push(format!(
            "hai lavorato {} principalmente sul progetto {}",
            human_duration(secs),
            p
        )),
        None => parts.push(format!(
            "hai lavorato {} in totale",
            human_duration(m.active_seconds)
        )),
    }

    let bugs = count_bugfix(input.commits);
    if bugs > 0 {
        parts.push(format!(
            "corretto {} bug",
            bugs
        ));
    }

    if !input.commits.is_empty() {
        parts.push(format!("effettuato {} commit", input.commits.len()));
    }

    let meetings = count_meetings(input.samples, input.meetings);
    if meetings > 0 {
        parts.push(format!("partecipato a {} meeting", meetings));
    }

    let body = join_natural(&parts);
    format!(
        "Oggi {}. Focus: {} su {} attivo ({} cambi di contesto, {} interruzioni).",
        body,
        human_duration(m.focus_seconds),
        human_duration(m.active_seconds),
        m.context_switches,
        m.interruptions,
    )
}

/// Unisce frammenti con virgole e una "e" finale, in italiano.
fn join_natural(parts: &[String]) -> String {
    match parts.len() {
        0 => String::new(),
        1 => parts[0].clone(),
        _ => {
            let (last, head) = parts.split_last().unwrap();
            format!("{} e {}", head.join(", "), last)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn s(app: &str, project: Option<&str>, cat: Category, secs: i64) -> ActivitySample {
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

    fn commit(msg: &str) -> GitCommit {
        GitCommit {
            repo: "pam".into(),
            hash: msg.into(),
            author: "me".into(),
            message: msg.into(),
            branch: "main".into(),
            project: Some("PAM".into()),
            at: Utc::now(),
            additions: 0,
            deletions: 0,
        }
    }

    #[test]
    fn riepilogo_giornata_tipica() {
        let samples = vec![
            s("Code", Some("PAM"), Category::Coding, 3 * 3600),
            s("Teams", None, Category::Communication, 1800),
            s("Teams", None, Category::Communication, 1200),
        ];
        let commits = vec![
            commit("fix: PAM-1 login"),
            commit("fix: PAM-2 retry"),
            commit("feat: PAM-3 ui"),
        ];
        let out = daily_summary(&SummaryInput {
            samples: &samples,
            commits: &commits,
            meetings: &[],
        });
        assert!(out.contains("progetto PAM"));
        assert!(out.contains("2 bug"));
        assert!(out.contains("3 commit"));
        assert!(out.contains("2 meeting"));
    }
}
