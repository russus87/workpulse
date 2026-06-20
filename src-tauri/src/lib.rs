//! Backend Tauri di WorkPulse: espone alla UI i dati gia' tracciati e aggregati
//! dal crate `workpulse-core`. Ogni comando e' un sottile adattatore che apre
//! la finestra temporale richiesta, interroga lo `Store` e converte gli errori
//! in stringhe leggibili per il frontend.

mod capture;
mod settings;
mod tracker;

use chrono::{DateTime, Datelike, Duration, Local, TimeZone, Utc};
use serde::Serialize;
use settings::Settings;
use std::sync::Arc;
use tauri::Manager;
use tracker::AppState;
use workpulse_core::aggregate;
use workpulse_core::model::{JournalEntry, ProductivityMetrics, UsageRow};
use workpulse_core::storage::Dimension;
use workpulse_core::summary::{self, SummaryInput};

/// Converte un errore in stringa per la UI.
fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Risolve un "periodo" testuale in un intervallo [from, to) in UTC, calcolato
/// sui confini locali del giorno (cosi' "oggi" e' il giorno dell'utente).
fn range(period: &str) -> (DateTime<Utc>, DateTime<Utc>) {
    let now = Local::now();
    let start_of_today = Local
        .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .unwrap();
    let (from_local, to_local) = match period {
        "week" => {
            let weekday = now.weekday().num_days_from_monday() as i64;
            (start_of_today - Duration::days(weekday), start_of_today + Duration::days(1))
        }
        "month" => {
            let first = Local
                .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
                .unwrap();
            (first, start_of_today + Duration::days(1))
        }
        // "today" e default
        _ => (start_of_today, start_of_today + Duration::days(1)),
    };
    (from_local.with_timezone(&Utc), to_local.with_timezone(&Utc))
}

fn parse_dimension(d: &str) -> Dimension {
    match d {
        "project" => Dimension::Project,
        "ticket" => Dimension::Ticket,
        "client" => Dimension::Client,
        "category" => Dimension::Category,
        _ => Dimension::App,
    }
}

/// Tempo per dimensione (app/project/ticket/client/category) in un periodo.
#[tauri::command]
fn usage_by(
    state: tauri::State<Arc<AppState>>,
    dimension: String,
    period: String,
) -> Result<Vec<UsageRow>, String> {
    let (from, to) = range(&period);
    let store = state.store.lock().map_err(err)?;
    store
        .usage_by(parse_dimension(&dimension), from, to)
        .map_err(err)
}

/// Metriche di produttivita' (focus, context switch, interruzioni) nel periodo.
#[tauri::command]
fn productivity(
    state: tauri::State<Arc<AppState>>,
    period: String,
) -> Result<ProductivityMetrics, String> {
    let (from, to) = range(&period);
    let store = state.store.lock().map_err(err)?;
    let samples = store.samples_between(from, to).map_err(err)?;
    Ok(aggregate::metrics(&samples))
}

/// Riepilogo in linguaggio naturale del periodo (default: oggi).
#[tauri::command]
fn ai_summary(state: tauri::State<Arc<AppState>>, period: String) -> Result<String, String> {
    let (from, to) = range(&period);
    let store = state.store.lock().map_err(err)?;
    let samples = store.samples_between(from, to).map_err(err)?;
    let commits = store.commits_between(from, to).map_err(err)?;
    Ok(summary::daily_summary(&SummaryInput {
        samples: &samples,
        commits: &commits,
    }))
}

/// Work Journal del periodo: una voce per progetto con tempo, ticket e commit.
#[tauri::command]
fn journal(state: tauri::State<Arc<AppState>>, period: String) -> Result<Vec<JournalEntry>, String> {
    let (from, to) = range(&period);
    let store = state.store.lock().map_err(err)?;
    let samples = store.samples_between(from, to).map_err(err)?;
    let commits = store.commits_between(from, to).map_err(err)?;
    let day = Local::now().format("%Y-%m-%d").to_string();
    Ok(workpulse_core::journal::build(&day, &samples, &commits))
}

/// Una riga di timesheet: giorno + ripartizione del tempo per progetto.
#[derive(Serialize)]
struct TimesheetDay {
    day: String,
    rows: Vec<UsageRow>,
    total_seconds: i64,
}

/// Timesheet del periodo, giorno per giorno, ripartito per progetto.
#[tauri::command]
fn timesheet(
    state: tauri::State<Arc<AppState>>,
    period: String,
) -> Result<Vec<TimesheetDay>, String> {
    let (from, to) = range(&period);
    let store = state.store.lock().map_err(err)?;
    let mut days = Vec::new();
    let mut cursor = from;
    while cursor < to {
        let next = cursor + Duration::days(1);
        let rows = store
            .usage_by(Dimension::Project, cursor, next)
            .map_err(err)?;
        let total_seconds = rows.iter().map(|r| r.seconds).sum();
        if total_seconds > 0 {
            days.push(TimesheetDay {
                day: cursor.with_timezone(&Local).format("%Y-%m-%d").to_string(),
                rows,
                total_seconds,
            });
        }
        cursor = next;
    }
    Ok(days)
}

/// Restituisce le impostazioni correnti.
#[tauri::command]
fn get_settings(state: tauri::State<Arc<AppState>>) -> Result<Settings, String> {
    Ok(state.settings.lock().map_err(err)?.clone())
}

/// Aggiorna e persiste le impostazioni (regole, repo, retention, intervallo).
#[tauri::command]
fn save_settings(
    state: tauri::State<Arc<AppState>>,
    new_settings: Settings,
) -> Result<(), String> {
    new_settings.save().map_err(err)?;
    *state.settings.lock().map_err(err)? = new_settings;
    Ok(())
}

/// Mette in pausa / riprende il tracciamento. Ritorna il nuovo stato.
#[tauri::command]
fn set_paused(state: tauri::State<Arc<AppState>>, paused: bool) -> Result<bool, String> {
    *state.paused.lock().map_err(err)? = paused;
    Ok(paused)
}

/// Importa subito i commit dai repo configurati (utile dopo aver aggiunto repo).
#[tauri::command]
fn sync_git(state: tauri::State<Arc<AppState>>) -> Result<(), String> {
    let s: Arc<AppState> = Arc::clone(&state);
    tracker::import_commits(&s);
    Ok(())
}

/// Cancella i dati piu' vecchi di `days` giorni (privacy / retention manuale).
#[tauri::command]
fn purge(state: tauri::State<Arc<AppState>>, days: i64) -> Result<usize, String> {
    let cutoff = Utc::now() - Duration::days(days.max(0));
    let store = state.store.lock().map_err(err)?;
    store.purge_before(cutoff).map_err(err)
}

/// Punto di ingresso dell'app Tauri.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let settings = Settings::load();
    let store = workpulse_core::storage::Store::open(
        settings::db_path().to_string_lossy().as_ref(),
    )
    .expect("impossibile aprire il database di WorkPulse");

    // Retention automatica all'avvio.
    if settings.retention_days > 0 {
        let cutoff = Utc::now() - Duration::days(settings.retention_days);
        let _ = store.purge_before(cutoff);
    }

    let state = Arc::new(AppState::new(store, settings));
    let state_for_tracker = Arc::clone(&state);

    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_process::init())
        .manage(Arc::clone(&state))
        .setup(move |_app| {
            // Avvia il tracciamento in background appena l'app e' pronta.
            tracker::start(Arc::clone(&state_for_tracker));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            usage_by,
            productivity,
            ai_summary,
            journal,
            timesheet,
            get_settings,
            save_settings,
            set_paused,
            sync_git,
            purge,
        ])
        .run(tauri::generate_context!())
        .expect("errore irreversibile all'avvio di WorkPulse");
}
