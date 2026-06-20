//! Tracker in background: campiona periodicamente la finestra attiva, classifica
//! l'attivita' e la persiste. Inoltre, a intervalli piu' larghi, importa i commit
//! dai repo Git configurati.
//!
//! Gira su un thread dedicato e comunica con il resto dell'app tramite `AppState`
//! (un `Store` protetto da Mutex). Nessun polling dalla UI: i dati sono gia' pronti.

use crate::capture;
use crate::settings::Settings;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use workpulse_core::classify::Classifier;
use workpulse_core::git;
use workpulse_core::storage::Store;

/// Stato condiviso tra la UI (comandi Tauri) e il thread di tracking.
pub struct AppState {
    pub store: Mutex<Store>,
    pub classifier: Classifier,
    pub settings: Mutex<Settings>,
    /// Flag per mettere in pausa la registrazione (privacy: "pausa tracciamento").
    pub paused: Mutex<bool>,
}

impl AppState {
    pub fn new(store: Store, settings: Settings) -> Self {
        let classifier = Classifier::new(settings.rules.clone());
        AppState {
            store: Mutex::new(store),
            classifier,
            settings: Mutex::new(settings),
            paused: Mutex::new(false),
        }
    }
}

/// Avvia il loop di campionamento su un thread in background.
pub fn start(state: Arc<AppState>) {
    thread::spawn(move || {
        let interval = {
            let s = state.settings.lock().unwrap();
            s.sample_seconds.max(5)
        };
        // Branch dell'ultimo repo "in primo piano": qui, per semplicita', usiamo
        // il primo repo configurato come contesto Git corrente.
        let mut ticks: u64 = 0;
        loop {
            thread::sleep(Duration::from_secs(interval as u64));
            ticks += 1;

            if *state.paused.lock().unwrap() {
                continue;
            }

            // Branch Git di contesto (best-effort sul primo repo tracciato).
            let git_branch = {
                let s = state.settings.lock().unwrap();
                s.git_repos
                    .first()
                    .and_then(|r| git::current_branch(r))
            };

            if let Some(snap) = capture::snapshot(false, git_branch) {
                let sample = state.classifier.classify(&snap, interval);
                if let Ok(store) = state.store.lock() {
                    let _ = store.insert_sample(&sample);
                }
            }

            // Ogni ~5 minuti importa i commit recenti dai repo configurati.
            if ticks % (300 / interval.max(1) as u64).max(1) == 0 {
                import_commits(&state);
            }
        }
    });
}

/// Importa i commit recenti (ultime 24h) dai repo configurati.
pub fn import_commits(state: &Arc<AppState>) {
    let (repos, email) = {
        let s = state.settings.lock().unwrap();
        (s.git_repos.clone(), s.author_email.clone())
    };
    for repo in repos {
        if let Ok(commits) = git::read_commits(&repo, &email, "1 day ago") {
            if let Ok(store) = state.store.lock() {
                for c in &commits {
                    let _ = store.upsert_commit(c);
                }
            }
        }
    }
}
