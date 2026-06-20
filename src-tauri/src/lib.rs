//! Backend Tauri di WorkPulse: espone alla UI i dati gia' tracciati e aggregati
//! dal crate `workpulse-core`. Ogni comando e' un sottile adattatore che apre
//! la finestra temporale richiesta, interroga lo `Store` e converte gli errori
//! in stringhe leggibili per il frontend.

mod capture;
mod graph;
mod llm;
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
use workpulse_core::billing::{self, BillItem};
use workpulse_core::devstats::{self, CodeTotals};
use workpulse_core::heatmap::{self, HeatCell};
use workpulse_core::model::{ActivitySample, JournalEntry, Meeting, ProductivityMetrics, UsageRow};
use workpulse_core::report::csv_line;
use workpulse_core::standup;
use workpulse_core::storage::Dimension;
use workpulse_core::suggest::{self, Suggestion};
use workpulse_core::summary::{self, SummaryInput};
use workpulse_core::trends::{self, Comparison, DayTotal};

/// Servizio usato nel portachiavi di sistema per la passphrase del DB.
const KEYRING_SERVICE: &str = "WorkPulse";
const KEYRING_USER: &str = "db-passphrase";

/// Legge la passphrase del DB dal portachiavi, se presente.
fn keyring_get() -> Option<String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .ok()?
        .get_password()
        .ok()
}

/// Salva la passphrase del DB nel portachiavi.
fn keyring_set(pass: &str) -> Result<(), String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(err)?
        .set_password(pass)
        .map_err(err)
}

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
    let meetings = store.meetings_between(from, to).map_err(err)?;
    Ok(summary::daily_summary(&SummaryInput {
        samples: &samples,
        commits: &commits,
        meetings: &meetings,
    }))
}

/// Meeting (da calendario) nel periodo.
#[tauri::command]
fn meetings(state: tauri::State<Arc<AppState>>, period: String) -> Result<Vec<Meeting>, String> {
    let (from, to) = range(&period);
    let store = state.store.lock().map_err(err)?;
    store.meetings_between(from, to).map_err(err)
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
    state.reload_classifier(); // applica subito eventuali nuove regole
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

// ---- Connettore Microsoft Graph (Outlook/Teams) ----

/// Avvia il device code flow: ritorna il codice/URL da mostrare all'utente.
#[tauri::command]
fn graph_start_auth(state: tauri::State<Arc<AppState>>) -> Result<graph::DeviceCode, String> {
    let (client_id, tenant) = {
        let s = state.settings.lock().map_err(err)?;
        (s.graph_client_id.clone(), s.graph_tenant.clone())
    };
    if client_id.is_empty() {
        return Err("Imposta prima il client_id dell'app Azure AD.".into());
    }
    graph::start_device_code(&client_id, &tenant)
}

/// Esegue un giro di polling del token. Ritorna "ok" | "pending" | messaggio errore.
#[tauri::command]
fn graph_poll_auth(
    state: tauri::State<Arc<AppState>>,
    device_code: String,
) -> Result<String, String> {
    let (client_id, tenant) = {
        let s = state.settings.lock().map_err(err)?;
        (s.graph_client_id.clone(), s.graph_tenant.clone())
    };
    match graph::poll_token(&client_id, &tenant, &device_code) {
        graph::Poll::Pending => Ok("pending".into()),
        graph::Poll::Failed(m) => Err(m),
        graph::Poll::Done(_access, refresh) => {
            let refresh = refresh.ok_or("nessun refresh token ricevuto")?;
            let mut s = state.settings.lock().map_err(err)?;
            s.graph_refresh_token = refresh;
            s.graph_enabled = true;
            s.save().map_err(err)?;
            Ok("ok".into())
        }
    }
}

/// Sincronizza i meeting dal calendario (ultimi 7 giorni + domani). Ritorna il numero importato.
#[tauri::command]
fn graph_sync(state: tauri::State<Arc<AppState>>) -> Result<usize, String> {
    let (client_id, tenant, refresh) = {
        let s = state.settings.lock().map_err(err)?;
        (s.graph_client_id.clone(), s.graph_tenant.clone(), s.graph_refresh_token.clone())
    };
    if refresh.is_empty() {
        return Err("Connettore non autorizzato. Esegui prima la connessione.".into());
    }
    let (access, new_refresh) = graph::refresh_access_token(&client_id, &tenant, &refresh)?;
    if let Some(nr) = new_refresh {
        let mut s = state.settings.lock().map_err(err)?;
        s.graph_refresh_token = nr;
        let _ = s.save();
    }
    let from = Utc::now() - Duration::days(7);
    let to = Utc::now() + Duration::days(1);
    let meetings = graph::fetch_meetings(&access, from, to)?;
    let store = state.store.lock().map_err(err)?;
    let mut n = 0;
    for m in &meetings {
        if store.upsert_meeting(m).is_ok() {
            n += 1;
        }
    }
    Ok(n)
}

/// Disconnette il connettore Graph (rimuove il refresh token salvato).
#[tauri::command]
fn graph_disconnect(state: tauri::State<Arc<AppState>>) -> Result<(), String> {
    let mut s = state.settings.lock().map_err(err)?;
    s.graph_refresh_token.clear();
    s.graph_enabled = false;
    s.save().map_err(err)
}

// ---- Fatturazione ----

#[derive(Serialize)]
struct BillingResult {
    items: Vec<BillItem>,
    total: f64,
    currency_hint: String,
}

/// Calcola la fatturazione per cliente nel periodo (tariffe + arrotondamento dalle impostazioni).
#[tauri::command]
fn billing(state: tauri::State<Arc<AppState>>, period: String) -> Result<BillingResult, String> {
    let (from, to) = range(&period);
    let (rates, round) = {
        let s = state.settings.lock().map_err(err)?;
        (s.rates.clone(), s.billing_round_minutes)
    };
    let store = state.store.lock().map_err(err)?;
    let rows = store.usage_by(Dimension::Client, from, to).map_err(err)?;
    let items = billing::bill(&rows, &rates, round);
    let total = billing::total(&items);
    Ok(BillingResult { items, total, currency_hint: "€".into() })
}

// ---- Standup / metriche dev ----

/// Genera il testo dello standup (Markdown) per il periodo.
#[tauri::command]
fn standup_text(state: tauri::State<Arc<AppState>>, period: String) -> Result<String, String> {
    let (from, to) = range(&period);
    let label = match period.as_str() {
        "week" => "Questa settimana",
        "month" => "Questo mese",
        _ => "Oggi",
    };
    let store = state.store.lock().map_err(err)?;
    let samples = store.samples_between(from, to).map_err(err)?;
    let commits = store.commits_between(from, to).map_err(err)?;
    let meetings = store.meetings_between(from, to).map_err(err)?;
    Ok(standup::standup(label, &samples, &commits, &meetings))
}

/// Tempo per linguaggio (dedotto dai titoli dell'editor) nel periodo.
#[tauri::command]
fn languages(state: tauri::State<Arc<AppState>>, period: String) -> Result<Vec<UsageRow>, String> {
    let (from, to) = range(&period);
    let store = state.store.lock().map_err(err)?;
    let samples = store.samples_between(from, to).map_err(err)?;
    Ok(devstats::by_language(&samples))
}

/// Totali di codice (commit, righe +/-) nel periodo.
#[tauri::command]
fn code_totals(state: tauri::State<Arc<AppState>>, period: String) -> Result<CodeTotals, String> {
    let (from, to) = range(&period);
    let store = state.store.lock().map_err(err)?;
    let commits = store.commits_between(from, to).map_err(err)?;
    Ok(devstats::code_totals(&commits))
}

/// Heatmap (giorno x ora) del periodo.
#[tauri::command]
fn heat(state: tauri::State<Arc<AppState>>, period: String) -> Result<Vec<HeatCell>, String> {
    let (from, to) = range(&period);
    let store = state.store.lock().map_err(err)?;
    let samples = store.samples_between(from, to).map_err(err)?;
    Ok(heatmap::heatmap(&samples))
}

/// Suggerimenti di regole (app/branch senza progetto) nel periodo.
#[tauri::command]
fn suggestions(state: tauri::State<Arc<AppState>>, period: String) -> Result<Vec<Suggestion>, String> {
    let (from, to) = range(&period);
    let store = state.store.lock().map_err(err)?;
    let samples = store.samples_between(from, to).map_err(err)?;
    Ok(suggest::suggest(&samples, 600))
}

// ---- Correzione manuale ----

/// Aggiorna i campi di un sample (progetto/ticket/cliente/idle). Stringa vuota = azzera.
#[tauri::command]
fn update_sample(
    state: tauri::State<Arc<AppState>>,
    id: i64,
    project: Option<String>,
    ticket: Option<String>,
    client: Option<String>,
    idle: Option<bool>,
) -> Result<(), String> {
    let store = state.store.lock().map_err(err)?;
    store
        .update_sample(id, project.as_deref(), ticket.as_deref(), client.as_deref(), idle)
        .map_err(err)
}

/// Riassegna in blocco tutti i sample di un'app a un progetto/cliente.
#[tauri::command]
fn reassign_app(
    state: tauri::State<Arc<AppState>>,
    app: String,
    project: String,
    client: Option<String>,
) -> Result<usize, String> {
    let store = state.store.lock().map_err(err)?;
    store.reassign_app(&app, &project, client.as_deref()).map_err(err)
}

/// Elimina un sample.
#[tauri::command]
fn delete_sample(state: tauri::State<Arc<AppState>>, id: i64) -> Result<(), String> {
    let store = state.store.lock().map_err(err)?;
    store.delete_sample(id).map_err(err)
}

/// Blocchi idle da riconciliare nel periodo (default >= 5 minuti).
#[tauri::command]
fn idle_blocks(
    state: tauri::State<Arc<AppState>>,
    period: String,
) -> Result<Vec<ActivitySample>, String> {
    let (from, to) = range(&period);
    let store = state.store.lock().map_err(err)?;
    store.idle_blocks(from, to, 300).map_err(err)
}

// ---- Focus / Pomodoro ----

/// Avvia una sessione di focus di `minutes` minuti (0 = usa la durata Pomodoro).
#[tauri::command]
fn focus_start(state: tauri::State<Arc<AppState>>, minutes: i64) -> Result<i64, String> {
    let m = if minutes > 0 {
        minutes
    } else {
        state.settings.lock().map_err(err)?.pomodoro_minutes
    };
    let until = Utc::now() + Duration::minutes(m);
    *state.focus_until.lock().map_err(err)? = Some(until);
    Ok(m)
}

/// Interrompe la sessione di focus.
#[tauri::command]
fn focus_stop(state: tauri::State<Arc<AppState>>) -> Result<(), String> {
    *state.focus_until.lock().map_err(err)? = None;
    Ok(())
}

/// Secondi rimanenti della sessione di focus (0 se non attiva).
#[tauri::command]
fn focus_status(state: tauri::State<Arc<AppState>>) -> Result<i64, String> {
    let until = *state.focus_until.lock().map_err(err)?;
    Ok(match until {
        Some(end) => (end - Utc::now()).num_seconds().max(0),
        None => 0,
    })
}

// ---- Insight LLM locale ----

/// Genera un insight settimanale con l'LLM locale (fallback: riepilogo a template).
#[tauri::command]
fn llm_insights(state: tauri::State<Arc<AppState>>, period: String) -> Result<String, String> {
    let (from, to) = range(&period);
    let (enabled, endpoint, model) = {
        let s = state.settings.lock().map_err(err)?;
        (s.llm_enabled, s.llm_endpoint.clone(), s.llm_model.clone())
    };

    // Base dati testuale (gia' aggregata, nessun titolo grezzo).
    let data = {
        let store = state.store.lock().map_err(err)?;
        let samples = store.samples_between(from, to).map_err(err)?;
        let commits = store.commits_between(from, to).map_err(err)?;
        let meetings = store.meetings_between(from, to).map_err(err)?;
        let by_project = store.usage_by(Dimension::Project, from, to).map_err(err)?;
        let m = aggregate::metrics(&samples);
        let mut s = String::new();
        s.push_str(&format!(
            "Tempo attivo: {}, focus: {}, context switch: {}, interruzioni: {}.\n",
            aggregate::human_duration(m.active_seconds),
            aggregate::human_duration(m.focus_seconds),
            m.context_switches,
            m.interruptions
        ));
        for r in by_project.iter().take(8) {
            s.push_str(&format!("- {}: {}\n", r.key, aggregate::human_duration(r.seconds)));
        }
        s.push_str(&format!("Commit: {}, meeting: {}.\n", commits.len(), meetings.len()));
        s
    };

    if !enabled {
        return Err("Insight LLM non abilitati. Attivali nelle impostazioni.".into());
    }
    let prompt = llm::weekly_prompt(&data);
    llm::generate(&endpoint, &model, &prompt)
}

// ---- Cifratura DB a riposo ----

/// Attiva la cifratura del DB: migra i dati in un file cifrato e salva la
/// passphrase nel portachiavi. Richiede il riavvio per completare.
#[tauri::command]
fn enable_db_encryption(
    state: tauri::State<Arc<AppState>>,
    passphrase: String,
) -> Result<String, String> {
    if passphrase.len() < 8 {
        return Err("La passphrase deve avere almeno 8 caratteri.".into());
    }
    {
        let s = state.settings.lock().map_err(err)?;
        if s.db_encrypted {
            return Err("Il database e' gia' cifrato.".into());
        }
    }
    let plain = settings::db_path();
    let enc = plain.with_extension("db.enc");
    workpulse_core::storage::Store::export_encrypted(
        plain.to_string_lossy().as_ref(),
        enc.to_string_lossy().as_ref(),
        &passphrase,
    )
    .map_err(err)?;
    keyring_set(&passphrase)?;
    {
        let mut s = state.settings.lock().map_err(err)?;
        s.db_encrypted = true;
        s.save().map_err(err)?;
    }
    Ok("Cifratura attivata. Riavvia WorkPulse per completare.".into())
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
    let db = settings::db_path();

    // Cifratura a riposo: se attiva, completa l'eventuale migrazione pendente
    // (file .db.enc creato dal comando di attivazione) e apri con la passphrase.
    let key: Option<String> = if settings.db_encrypted {
        let enc = db.with_extension("db.enc");
        if enc.exists() {
            // Sostituisce il DB in chiaro con quello cifrato.
            let _ = std::fs::remove_file(&db);
            let _ = std::fs::rename(&enc, &db);
        }
        keyring_get()
    } else {
        None
    };

    let store = workpulse_core::storage::Store::open_with_key(
        db.to_string_lossy().as_ref(),
        key.as_deref(),
    )
    .expect("impossibile aprire il database di WorkPulse (passphrase errata?)");

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
            meetings,
            graph_start_auth,
            graph_poll_auth,
            graph_sync,
            graph_disconnect,
            billing,
            standup_text,
            languages,
            code_totals,
            heat,
            suggestions,
            update_sample,
            reassign_app,
            delete_sample,
            idle_blocks,
            focus_start,
            focus_stop,
            focus_status,
            llm_insights,
            enable_db_encryption,
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
