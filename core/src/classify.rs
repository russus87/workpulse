//! Classificazione delle attivita': da uno `WindowSnapshot` grezzo a un
//! `ActivitySample` con categoria, progetto, ticket e cliente.
//!
//! La logica e' guidata da regole semplici e trasparenti (niente "magia"):
//!   - la categoria deriva dal nome dell'app;
//!   - ticket e progetto si riconoscono con una regex tipo `ABC-123`;
//!   - il cliente si ricava mappando il progetto via `Rules`.
//!
//! Le regole sono dati (configurabili dall'utente), non codice: cosi' ognuno
//! puo' adattare WorkPulse al proprio modo di lavorare senza ricompilare.

use crate::model::{ActivitySample, Category, WindowSnapshot};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Regole di classificazione configurabili dall'utente.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rules {
    /// Nomi di app (lowercase) considerate coding/IDE/terminale.
    pub coding_apps: Vec<String>,
    /// Nomi di app considerate comunicazione/meeting.
    pub communication_apps: Vec<String>,
    /// Nomi di app considerate documenti/scrittura.
    pub document_apps: Vec<String>,
    /// Nomi di app considerate browser.
    pub browser_apps: Vec<String>,
    /// Mappa codice-progetto -> nome cliente (es. "PAM" -> "Acme S.p.A.").
    pub project_to_client: HashMap<String, String>,
}

impl Default for Rules {
    fn default() -> Self {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect();
        Rules {
            coding_apps: s(&[
                "code", "code - insiders", "vscodium", "intellij idea", "pycharm",
                "goland", "rustrover", "webstorm", "clion", "nvim", "vim", "emacs",
                "windowsterminal", "wezterm", "alacritty", "kitty", "konsole",
                "gnome-terminal", "terminal", "iterm2", "sublime_text", "zed",
            ]),
            communication_apps: s(&[
                "teams", "ms-teams", "slack", "outlook", "discord", "zoom",
                "webex", "skype", "telegram",
            ]),
            document_apps: s(&[
                "winword", "word", "excel", "powerpoint", "libreoffice",
                "soffice", "acrobat", "obsidian", "notion", "onenote",
            ]),
            browser_apps: s(&[
                "firefox", "chrome", "google chrome", "chromium", "msedge",
                "edge", "safari", "brave", "vivaldi", "opera",
            ]),
            project_to_client: HashMap::new(),
        }
    }
}

/// Classificatore: compila una volta le regex e applica le regole.
pub struct Classifier {
    rules: Rules,
    /// Riconosce token tipo `PAM-1423` (codice progetto + numero ticket).
    ticket_re: Regex,
}

impl Classifier {
    pub fn new(rules: Rules) -> Self {
        // Codice progetto: 2-10 lettere maiuscole; ticket: trattino + cifre.
        let ticket_re = Regex::new(r"\b([A-Z][A-Z0-9]{1,9})-(\d{1,6})\b").unwrap();
        Classifier { rules, ticket_re }
    }

    /// Determina la categoria a partire dal nome dell'app.
    fn category_of(&self, app_lower: &str) -> Category {
        let any = |list: &[String]| list.iter().any(|a| app_lower.contains(a.as_str()));
        if any(&self.rules.coding_apps) {
            Category::Coding
        } else if any(&self.rules.communication_apps) {
            Category::Communication
        } else if any(&self.rules.document_apps) {
            Category::Documents
        } else if any(&self.rules.browser_apps) {
            Category::Browsing
        } else {
            Category::Other
        }
    }

    /// Estrae (progetto, ticket) dal titolo della finestra, dal branch o dall'URL.
    fn extract_project_ticket(
        &self,
        title: &str,
        branch: Option<&str>,
        url: Option<&str>,
    ) -> (Option<String>, Option<String>) {
        for hay in [Some(title), branch, url].into_iter().flatten() {
            if let Some(c) = self.ticket_re.captures(hay) {
                let project = c.get(1).unwrap().as_str().to_string();
                let ticket = format!("{}-{}", &project, c.get(2).unwrap().as_str());
                return (Some(project), Some(ticket));
            }
        }
        (None, None)
    }

    /// Trasforma uno snapshot in un sample con durata `seconds`.
    pub fn classify(&self, snap: &WindowSnapshot, seconds: i64) -> ActivitySample {
        let app_lower = snap.app.to_lowercase();
        let category = self.category_of(&app_lower);
        let (project, ticket) = self.extract_project_ticket(
            &snap.title,
            snap.git_branch.as_deref(),
            snap.url.as_deref(),
        );
        let client = project
            .as_ref()
            .and_then(|p| self.rules.project_to_client.get(p).cloned());

        ActivitySample {
            id: None,
            app: snap.app.clone(),
            title: snap.title.clone(),
            url: snap.url.clone(),
            category,
            project,
            ticket,
            client,
            git_branch: snap.git_branch.clone(),
            start: snap.at,
            seconds,
            idle: snap.idle,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn snap(app: &str, title: &str) -> WindowSnapshot {
        WindowSnapshot {
            app: app.into(),
            title: title.into(),
            url: None,
            git_branch: None,
            idle: false,
            at: Utc::now(),
        }
    }

    #[test]
    fn riconosce_categoria_e_ticket() {
        let mut rules = Rules::default();
        rules
            .project_to_client
            .insert("PAM".into(), "Acme S.p.A.".into());
        let c = Classifier::new(rules);

        let s = c.classify(&snap("Code", "fix(login): PAM-1423 retry — main.rs"), 60);
        assert_eq!(s.category, Category::Coding);
        assert_eq!(s.project.as_deref(), Some("PAM"));
        assert_eq!(s.ticket.as_deref(), Some("PAM-1423"));
        assert_eq!(s.client.as_deref(), Some("Acme S.p.A."));
    }

    #[test]
    fn meeting_e_comunicazione() {
        let c = Classifier::new(Rules::default());
        let s = c.classify(&snap("Teams", "Daily standup"), 30);
        assert_eq!(s.category, Category::Communication);
        assert!(s.project.is_none());
    }
}
