//! Layer di fatturazione: trasforma il tempo tracciato in importi fatturabili.
//!
//! Puro e testabile: prende righe di utilizzo (`UsageRow`, tipicamente per
//! cliente o progetto), una tabella tariffe e un arrotondamento, e produce le
//! voci con ore arrotondate e importo. Niente valuta hard-coded: l'UI formatta.

use crate::model::UsageRow;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tariffe orarie: una di default + override per chiave (cliente o progetto).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Rates {
    pub default_hourly: f64,
    pub per_key: HashMap<String, f64>,
}

impl Rates {
    fn rate_for(&self, key: &str) -> f64 {
        *self.per_key.get(key).unwrap_or(&self.default_hourly)
    }
}

/// Una voce fatturabile calcolata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BillItem {
    pub key: String,
    /// Secondi effettivamente tracciati.
    pub seconds: i64,
    /// Secondi dopo l'arrotondamento (la base di fatturazione).
    pub billable_seconds: i64,
    pub hourly_rate: f64,
    pub amount: f64,
}

/// Calcola le voci fatturabili. `round_minutes` = incremento minimo (0 = nessuno):
/// ogni voce viene arrotondata per eccesso al multiplo piu' vicino.
pub fn bill(rows: &[UsageRow], rates: &Rates, round_minutes: i64) -> Vec<BillItem> {
    let step = round_minutes.max(0) * 60;
    rows.iter()
        .map(|r| {
            let billable_seconds = if step > 0 {
                ((r.seconds + step - 1) / step) * step
            } else {
                r.seconds
            };
            let hourly_rate = rates.rate_for(&r.key);
            let amount = billable_seconds as f64 / 3600.0 * hourly_rate;
            BillItem {
                key: r.key.clone(),
                seconds: r.seconds,
                billable_seconds,
                hourly_rate,
                amount: (amount * 100.0).round() / 100.0,
            }
        })
        .collect()
}

/// Totale fatturabile di un insieme di voci.
pub fn total(items: &[BillItem]) -> f64 {
    (items.iter().map(|i| i.amount).sum::<f64>() * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(k: &str, s: i64) -> UsageRow {
        UsageRow { key: k.into(), seconds: s }
    }

    #[test]
    fn arrotonda_e_calcola_importo() {
        let mut rates = Rates { default_hourly: 50.0, per_key: HashMap::new() };
        rates.per_key.insert("Acme".into(), 80.0);
        // 50 minuti Acme -> arrotondati a 60 (round 15) = 1h * 80 = 80.0
        let items = bill(&[row("Acme", 50 * 60), row("Beta", 30 * 60)], &rates, 15);
        assert_eq!(items[0].billable_seconds, 3600);
        assert_eq!(items[0].amount, 80.0);
        // Beta 30m -> resta 30m, default 50/h = 25.0
        assert_eq!(items[1].amount, 25.0);
        assert_eq!(total(&items), 105.0);
    }

    #[test]
    fn nessun_arrotondamento() {
        let rates = Rates { default_hourly: 60.0, per_key: HashMap::new() };
        let items = bill(&[row("X", 1800)], &rates, 0);
        assert_eq!(items[0].billable_seconds, 1800);
        assert_eq!(items[0].amount, 30.0);
    }
}
