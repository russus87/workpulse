// Evita una finestra di console extra su Windows in release. NON rimuovere.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
  workpulse_lib::run();
}
