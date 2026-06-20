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
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;
use tauri_plugin_autostart::ManagerExt;
use tracker::AppState;
use workpulse_core::aggregate;
use workpulse_core::model::{JournalEntry, ProductivityMetrics, UsageRow};
use workpulse_core::report::csv_line;
use workpulse_core::storage::Dimension;
use workpulse_core::summary::{self, SummaryInput};
use workpulse_core::trends::{self, Comparison, DayTotal};

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

/// Costruisce le righe di timesheet (riusato da `timesheet` ed `export_csv`).
fn timesheet_rows(
    state: &tauri::State<Arc<AppState>>,
    period: &str,
) -> Result<Vec<TimesheetDay>, String> {
    let (from, to) = range(period);
    let store = state.store.lock().map_err(err)?;
    let mut days = Vec::new();
    let mut cursor = from;
    while cursor < to {
        let next = cursor + Duration::days(1);
        let rows = store.usage_by(Dimension::Project, cursor, next).map_err(err)?;
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

/// Timesheet del periodo, giorno per giorno, ripartito per progetto.
#[tauri::command]
fn timesheet(
    state: tauri::State<Arc<AppState>>,
    period: String,
) -> Result<Vec<TimesheetDay>, String> {
    timesheet_rows(&state, &period)
}

/// Esporta il timesheet del periodo in formato CSV (giorno, progetto, ore, secondi).
#[tauri::command]
fn export_csv(state: tauri::State<Arc<AppState>>, period: String) -> Result<String, String> {
    let days = timesheet_rows(&state, &period)?;
    let mut out = String::new();
    out.push_str(&csv_line(&[
        "giorno".into(),
        "progetto".into(),
        "ore".into(),
        "secondi".into(),
    ]));
    out.push('\n');
    for d in days {
        for r in d.rows {
            out.push_str(&csv_line(&[
                d.day.clone(),
                r.key.clone(),
                aggregate::human_duration(r.seconds),
                r.seconds.to_string(),
            ]));
            out.push('\n');
        }
    }
    Ok(out)
}

/// Salva un testo (es. l'export CSV) sul percorso scelto dall'utente.
#[tauri::command]
fn save_text(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(err)
}

/// Serie giornaliera (attivo/focus) del periodo, per i grafici storici.
#[tauri::command]
fn daily_trend(state: tauri::State<Arc<AppState>>, period: String) -> Result<Vec<DayTotal>, String> {
    let (from, to) = range(&period);
    let store = state.store.lock().map_err(err)?;
    let samples = store.samples_between(from, to).map_err(err)?;
    Ok(trends::daily(&samples))
}

/// Confronto temporale: periodo corrente vs precedente equivalente.
#[derive(Serialize)]
struct PeriodComparison {
    active: Comparison,
    focus: Comparison,
}

#[tauri::command]
fn compare_periods(
    state: tauri::State<Arc<AppState>>,
    period: String,
) -> Result<PeriodComparison, String> {
    let (from, to) = range(&period);
    let span = to - from;
    let (pfrom, pto) = (from - span, from);
    let store = state.store.lock().map_err(err)?;
    let cur = aggregate::metrics(&store.samples_between(from, to).map_err(err)?);
    let prev = aggregate::metrics(&store.samples_between(pfrom, pto).map_err(err)?);
    Ok(PeriodComparison {
        active: Comparison::new(cur.active_seconds, prev.active_seconds),
        focus: Comparison::new(cur.focus_seconds, prev.focus_seconds),
    })
}

/// Restituisce le impostazioni correnti.
#[tauri::command]
fn get_settings(state: tauri::State<Arc<AppState>>) -> Result<Settings, String> {
    Ok(state.settings.lock().map_err(err)?.clone())
}

/// Aggiorna e persiste le impostazioni (regole, repo, retention, intervallo).
#[tauri::command]
fn save_settings(
    app: tauri::AppHandle,
    state: tauri::State<Arc<AppState>>,
    new_settings: Settings,
) -> Result<(), String> {
    apply_autostart(&app, new_settings.autostart);
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

/// Abilita/disabilita l'avvio automatico al login.
fn apply_autostart(app: &tauri::AppHandle, enable: bool) {
    let mgr = app.autolaunch();
    let _ = if enable { mgr.enable() } else { mgr.disable() };
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

    let want_autostart = settings.autostart;
    let state = Arc::new(AppState::new(store, settings));
    let state_for_tracker = Arc::clone(&state);

    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(Arc::clone(&state))
        .setup(move |app| {
            // Allinea l'autostart alle impostazioni salvate.
            apply_autostart(&app.handle().clone(), want_autostart);

            // Tray icon con menu rapido.
            build_tray(app)?;

            // Avvia il tracciamento in background appena l'app e' pronta.
            tracker::start(app.handle().clone(), Arc::clone(&state_for_tracker));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            usage_by,
            productivity,
            ai_summary,
            journal,
            timesheet,
            export_csv,
            save_text,
            daily_trend,
            compare_periods,
            get_settings,
            save_settings,
            set_paused,
            sync_git,
            purge,
        ])
        .run(tauri::generate_context!())
        .expect("errore irreversibile all'avvio di WorkPulse");
}

/// Costruisce la tray icon con menu: Mostra, Riepilogo, Pausa, Esci.
fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Mostra WorkPulse", true, None::<&str>)?;
    let summary = MenuItem::with_id(app, "summary", "Riepilogo di oggi", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pausa / Riprendi tracking", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Esci", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &summary, &pause, &quit])?;

    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("WorkPulse")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            "summary" => {
                use tauri_plugin_notification::NotificationExt;
                let state = app.state::<Arc<AppState>>().inner().clone();
                let text = tracker::today_summary_text(&state);
                let _ = app
                    .notification()
                    .builder()
                    .title("WorkPulse — riepilogo di oggi")
                    .body(&text)
                    .show();
            }
            "pause" => {
                let state = app.state::<Arc<AppState>>().inner().clone();
                let lock = state.paused.lock();
                if let Ok(mut p) = lock {
                    *p = !*p;
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}
