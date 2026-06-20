//! Apprendimento regole: analizza i sample e suggerisce mappature mancanti.
//! Es. "passi molto tempo su questa app/branch senza progetto assegnato".

use crate::model::ActivitySample;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Suggestion {
    /// "app" oppure "branch".
    pub kind: String,
    /// Valore osservato (nome app o branch) senza progetto.
    pub key: String,
    pub seconds: i64,
    pub message: String,
}

/// Suggerisce mappature per app/branch con tempo significativo ma senza progetto.
/// `min_seconds` filtra il rumore.
pub fn suggest(samples: &[ActivitySample], min_seconds: i64) -> Vec<Suggestion> {
    let mut app_secs: HashMap<String, i64> = HashMap::new();
    let mut branch_secs: HashMap<String, i64> = HashMap::new();
    for s in samples.iter().filter(|s| !s.idle && s.project.is_none()) {
        *app_secs.entry(s.app.clone()).or_default() += s.seconds;
        if let Some(b) = &s.git_branch {
            *branch_secs.entry(b.clone()).or_default() += s.seconds;
        }
    }

    let mut out = Vec::new();
    for (app, secs) in app_secs {
        if secs >= min_seconds {
            out.push(Suggestion {
                kind: "app".into(),
                key: app.clone(),
                seconds: secs,
                message: format!("Molto tempo su '{app}' senza progetto: vuoi mapparlo?"),
            });
        }
    }
    for (branch, secs) in branch_secs {
        if secs >= min_seconds {
            out.push(Suggestion {
                kind: "branch".into(),
                key: branch.clone(),
                seconds: secs,
                message: format!("Branch '{branch}' senza progetto riconosciuto."),
            });
        }
    }
    out.sort_by(|a, b| b.seconds.cmp(&a.seconds));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Category;
    use chrono::Utc;

    fn s(app: &str, project: Option<&str>, secs: i64) -> ActivitySample {
        ActivitySample {
            id: None,
            app: app.into(),
            title: String::new(),
            url: None,
            category: Category::Coding,
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
    fn suggerisce_app_non_mappata() {
        let samples = vec![
            s("CustomTool", None, 3600),
            s("Code", Some("PAM"), 3600),
        ];
        let sug = suggest(&samples, 600);
        assert_eq!(sug.len(), 1);
        assert_eq!(sug[0].key, "CustomTool");
    }
}
