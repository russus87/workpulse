//! Tracker in background: campiona periodicamente la finestra attiva, classifica
//! l'attivita' e la persiste. Marca i campioni come idle quando l'utente e'
//! inattivo, importa i commit dai repo Git e invia la notifica di riepilogo a
//! fine giornata.
//!
//! Gira su un thread dedicato e comunica con il resto dell'app tramite `AppState`
//! (un `Store` protetto da Mutex) e l'`AppHandle` di Tauri (per le notifiche).

use crate::capture;
use crate::settings::Settings;
use chrono::{Datelike, Local, TimeZone, Timelike, Utc};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;
use workpulse_core::classify::Classifier;
use workpulse_core::git;
use workpulse_core::storage::Store;
use workpulse_core::summary::{self, SummaryInput};

/// Stato condiviso tra la UI (comandi Tauri) e il thread di tracking.
pub struct AppState {
    pub store: Mutex<Store>,
    pub classifier: Classifier,
    pub settings: Mutex<Settings>,
    /// Flag per mettere in pausa la registrazione (privacy: "pausa tracciamento").
    pub paused: Mutex<bool>,
    /// Ultimo giorno (YYYY-MM-DD) per cui e' stato inviato il riepilogo.
    pub last_summary_day: Mutex<Option<String>>,
}

impl AppState {
    pub fn new(store: Store, settings: Settings) -> Self {
        let classifier = Classifier::new(settings.rules.clone());
        AppState {
            store: Mutex::new(store),
            classifier,
            settings: Mutex::new(settings),
            paused: Mutex::new(false),
            last_summary_day: Mutex::new(None),
        }
    }
}

/// Avvia il loop di campionamento su un thread in background.
pub fn start(app: AppHandle, state: Arc<AppState>) {
    thread::spawn(move || {
        let interval = {
            let s = state.settings.lock().unwrap();
            s.sample_seconds.max(5)
        };
        let mut ticks: u64 = 0;
        loop {
            thread::sleep(Duration::from_secs(interval as u64));
            ticks += 1;

            if *state.paused.lock().unwrap() {
                continue;
            }

            let (git_repo0, idle_threshold) = {
                let s = state.settings.lock().unwrap();
                (s.git_repos.first().cloned(), s.idle_threshold_seconds)
            };

            // Idle reale: l'utente e' inattivo oltre la soglia?
            let idle = capture::idle_seconds()
                .map(|sec| sec >= idle_threshold)
                .unwrap_or(false);

            let git_branch = git_repo0.as_deref().and_then(git::current_branch);

            if let Some(snap) = capture::snapshot(idle, git_branch) {
                let sample = state.classifier.classify(&snap, interval);
                if let Ok(store) = state.store.lock() {
                    let _ = store.insert_sample(&sample);
                }
            }

            // Ogni ~5 minuti importa i commit recenti dai repo configurati.
            if ticks % (300 / interval.max(1) as u64).max(1) == 0 {
                import_commits(&state);
            }

            maybe_daily_summary(&app, &state);
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

/// Genera il testo del riepilogo di oggi (riusato da tracker e comandi).
pub fn today_summary_text(state: &Arc<AppState>) -> String {
    let now = Local::now();
    let from = Local
        .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .unwrap()
        .with_timezone(&Utc);
    let to = Utc::now();
    let store = match state.store.lock() {
        Ok(s) => s,
        Err(_) => return String::new(),
    };
    let samples = store.samples_between(from, to).unwrap_or_default();
    let commits = store.commits_between(from, to).unwrap_or_default();
    let meetings = store.meetings_between(from, to).unwrap_or_default();
    summary::daily_summary(&SummaryInput {
        samples: &samples,
        commits: &commits,
        meetings: &meetings,
    })
}

/// Invia la notifica di riepilogo una volta al giorno, dopo l'ora configurata.
fn maybe_daily_summary(app: &AppHandle, state: &Arc<AppState>) {
    let (enabled, hour) = {
        let s = state.settings.lock().unwrap();
        (s.daily_summary, s.daily_summary_hour)
    };
    if !enabled {
        return;
    }
    let now = Local::now();
    if now.hour() < hour {
        return;
    }
    let today = now.format("%Y-%m-%d").to_string();
    {
        let last = state.last_summary_day.lock().unwrap();
        if last.as_deref() == Some(today.as_str()) {
            return;
        }
    }
    let text = today_summary_text(state);
    let _ = app
        .notification()
        .builder()
        .title("WorkPulse — riepilogo di oggi")
        .body(&text)
        .show();
    *state.last_summary_day.lock().unwrap() = Some(today);
}
