//! Tracker in background: campiona periodicamente la finestra attiva, classifica
//! l'attivita' e la persiste. Marca i campioni come idle quando l'utente e'
//! inattivo, importa i commit dai repo Git e invia la notifica di riepilogo a
//! fine giornata.
//!
//! Gira su un thread dedicato e comunica con il resto dell'app tramite `AppState`
//! (un `Store` protetto da Mutex) e l'`AppHandle` di Tauri (per le notifiche).

use crate::capture;
use crate::graph;
use crate::settings::Settings;
use chrono::{Datelike, Local, TimeZone, Timelike, Utc};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;
use workpulse_core::classify::Classifier;
use workpulse_core::git;
use workpulse_core::model::Category;
use workpulse_core::storage::Store;
use workpulse_core::summary::{self, SummaryInput};

/// Stato condiviso tra la UI (comandi Tauri) e il thread di tracking.
pub struct AppState {
    pub store: Mutex<Store>,
    pub classifier: Mutex<Classifier>,
    pub settings: Mutex<Settings>,
    /// Flag per mettere in pausa la registrazione (privacy: "pausa tracciamento").
    pub paused: Mutex<bool>,
    /// Ultimo giorno (YYYY-MM-DD) per cui e' stato inviato il riepilogo.
    pub last_summary_day: Mutex<Option<String>>,
    /// Fine di una sessione di focus in corso (Pomodoro), se attiva.
    pub focus_until: Mutex<Option<chrono::DateTime<Utc>>>,
    /// Ultimo giorno in cui e' stato inviato il nudge "troppa comunicazione".
    pub last_comm_nudge_day: Mutex<Option<String>>,
    /// Access token Graph in cache con relativa scadenza (per il polling presence).
    pub graph_access: Mutex<Option<(String, chrono::DateTime<Utc>)>>,
}

impl AppState {
    pub fn new(store: Store, settings: Settings) -> Self {
        let classifier = Classifier::new(settings.rules.clone());
        AppState {
            store: Mutex::new(store),
            classifier: Mutex::new(classifier),
            settings: Mutex::new(settings),
            paused: Mutex::new(false),
            last_summary_day: Mutex::new(None),
            focus_until: Mutex::new(None),
            last_comm_nudge_day: Mutex::new(None),
            graph_access: Mutex::new(None),
        }
    }

    /// Aggiorna il classificatore quando cambiano le regole.
    pub fn reload_classifier(&self) {
        let rules = self.settings.lock().unwrap().rules.clone();
        *self.classifier.lock().unwrap() = Classifier::new(rules);
    }
}

/// Rileva se il titolo finestra indica una sessione di navigazione privata.
fn is_private_window(title: &str) -> bool {
    let t = title.to_lowercase();
    ["incognito", "inprivate", "private browsing", "navigazione in incognito", "(private"]
        .iter()
        .any(|m| t.contains(m))
}

/// Invia una notifica best-effort.
fn notify(app: &AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

/// Avvia il loop di campionamento su un thread in background.
pub fn start(app: AppHandle, state: Arc<AppState>) {
    thread::spawn(move || {
        let interval = {
            let s = state.settings.lock().unwrap();
            s.sample_seconds.max(5)
        };
        let mut ticks: u64 = 0;
        let mut active_streak: i64 = 0; // secondi attivi consecutivi senza pausa
        let mut focus_done_notified = true;
        let mut presence_accum: i64 = 0; // secondi accumulati per il polling presence
        loop {
            thread::sleep(Duration::from_secs(interval as u64));
            ticks += 1;

            if *state.paused.lock().unwrap() {
                continue;
            }

            let (git_repo0, idle_threshold, personal, private_autopause, no_break, comm_limit) = {
                let s = state.settings.lock().unwrap();
                (
                    s.git_repos.first().cloned(),
                    s.idle_threshold_seconds,
                    s.personal_apps.clone(),
                    s.private_autopause,
                    s.nudge_no_break_minutes,
                    s.nudge_comm_minutes,
                )
            };

            // Idle reale: l'utente e' inattivo oltre la soglia?
            let idle = capture::idle_seconds()
                .map(|sec| sec >= idle_threshold)
                .unwrap_or(false);

            let git_branch = git_repo0.as_deref().and_then(git::current_branch);

            if let Some(snap) = capture::snapshot(idle, git_branch) {
                // Auto-pausa privacy: app personali o finestre in incognito.
                let app_lower = snap.app.to_lowercase();
                let personal_hit = personal.iter().any(|p| app_lower.contains(&p.to_lowercase()));
                let private_hit = private_autopause && is_private_window(&snap.title);
                if !(personal_hit || private_hit) {
                    let sample = state.classifier.lock().unwrap().classify(&snap, interval);
                    if let Ok(store) = state.store.lock() {
                        let _ = store.insert_sample(&sample);
                    }
                }

                // Streak di attivita' per il nudge "fai una pausa".
                if idle {
                    active_streak = 0;
                } else {
                    active_streak += interval;
                    if no_break > 0 && active_streak >= no_break * 60 {
                        notify(&app, "WorkPulse — pausa?", &format!(
                            "Stai lavorando da {} senza pause.",
                            workpulse_core::aggregate::human_duration(active_streak)
                        ));
                        active_streak = 0;
                    }
                }
            }

            // Presence Teams: polling ogni ~60s (token in cache).
            presence_accum += interval;
            if presence_accum >= 60 {
                poll_presence(&state, presence_accum);
                presence_accum = 0;
            }

            // Nudge "troppa comunicazione" (una volta al giorno).
            maybe_comm_nudge(&app, &state, comm_limit);

            // Fine sessione di focus.
            focus_done_notified = check_focus_end(&app, &state, focus_done_notified);

            // Ogni ~5 minuti importa i commit recenti dai repo configurati.
            if ticks % (300 / interval.max(1) as u64).max(1) == 0 {
                import_commits(&state);
            }

            maybe_daily_summary(&app, &state);
        }
    });
}

/// Restituisce un access token Graph valido (dalla cache o via refresh).
/// Aggiorna anche il refresh token salvato se Microsoft ne emette uno nuovo.
pub fn graph_access_token(state: &Arc<AppState>) -> Option<String> {
    {
        let g = state.graph_access.lock().unwrap();
        if let Some((t, exp)) = &*g {
            if *exp > Utc::now() + chrono::Duration::seconds(60) {
                return Some(t.clone());
            }
        }
    }
    let (cid, tenant, refresh) = {
        let s = state.settings.lock().unwrap();
        (s.graph_client_id.clone(), s.graph_tenant.clone(), s.graph_refresh_token.clone())
    };
    if refresh.is_empty() {
        return None;
    }
    match graph::refresh_access_token(&cid, &tenant, &refresh) {
        Ok((access, new_refresh)) => {
            if let Some(nr) = new_refresh {
                let mut s = state.settings.lock().unwrap();
                s.graph_refresh_token = nr;
                let _ = s.save();
            }
            let exp = Utc::now() + chrono::Duration::minutes(50);
            *state.graph_access.lock().unwrap() = Some((access.clone(), exp));
            Some(access)
        }
        Err(_) => None,
    }
}

/// Registra la presence Teams corrente (best-effort) per `seconds`.
fn poll_presence(state: &Arc<AppState>, seconds: i64) {
    let (connected, track) = {
        let s = state.settings.lock().unwrap();
        (!s.graph_refresh_token.is_empty(), s.track_presence)
    };
    if !(connected && track) {
        return;
    }
    if let Some(token) = graph_access_token(state) {
        if let Ok((avail, activity)) = graph::fetch_presence(&token) {
            if let Ok(store) = state.store.lock() {
                let _ = store.insert_presence(&avail, &activity, Utc::now(), seconds);
            }
        }
    }
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

/// Inizio della giornata locale, in UTC.
fn start_of_today_utc() -> chrono::DateTime<Utc> {
    let now = Local::now();
    Local
        .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .unwrap()
        .with_timezone(&Utc)
}

/// Nudge "troppa comunicazione oggi", al massimo una volta al giorno.
fn maybe_comm_nudge(app: &AppHandle, state: &Arc<AppState>, comm_limit_minutes: i64) {
    if comm_limit_minutes <= 0 {
        return;
    }
    let today = Local::now().format("%Y-%m-%d").to_string();
    if state.last_comm_nudge_day.lock().unwrap().as_deref() == Some(today.as_str()) {
        return;
    }
    let comm_seconds = {
        let store = match state.store.lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        let samples = store
            .samples_between(start_of_today_utc(), Utc::now())
            .unwrap_or_default();
        samples
            .iter()
            .filter(|s| !s.idle && matches!(s.category, Category::Communication))
            .map(|s| s.seconds)
            .sum::<i64>()
    };
    if comm_seconds >= comm_limit_minutes * 60 {
        notify(
            app,
            "WorkPulse — comunicazione",
            &format!(
                "Oggi {} in comunicazione/meeting. Recupera un blocco di focus?",
                workpulse_core::aggregate::human_duration(comm_seconds)
            ),
        );
        *state.last_comm_nudge_day.lock().unwrap() = Some(today);
    }
}

/// Notifica la fine di una sessione di focus. Ritorna il nuovo flag "notificato".
fn check_focus_end(app: &AppHandle, state: &Arc<AppState>, notified: bool) -> bool {
    let mut guard = state.focus_until.lock().unwrap();
    match *guard {
        Some(end) => {
            if Utc::now() >= end {
                if !notified {
                    notify(
                        app,
                        "WorkPulse — focus completato",
                        "Sessione di focus terminata. Prenditi una pausa.",
                    );
                }
                *guard = None;
                true
            } else {
                false
            }
        }
        None => true,
    }
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
