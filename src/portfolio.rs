use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Holding {
    pub symbol: String,
    pub shares: f64,
    pub avg_cost: f64,
    pub added_at: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Portfolio {
    pub holdings: Vec<Holding>,
}

impl Portfolio {
    pub fn load() -> Result<Self> {
        crate::store::load("portfolio.json")
    }

    pub fn save(&self) -> Result<()> {
        crate::store::save("portfolio.json", self, false)
    }

    pub fn add(&mut self, symbol: &str, shares: f64, cost: f64) {
        // If already holding, average the cost
        let symbol_upper = symbol.to_uppercase();
        if let Some(h) = self.holdings.iter_mut().find(|h| h.symbol == symbol_upper) {
            let total_cost = h.shares * h.avg_cost + shares * cost;
            h.shares += shares;
            h.avg_cost = total_cost / h.shares;
        } else {
            self.holdings.push(Holding {
                symbol: symbol_upper,
                shares,
                avg_cost: cost,
                added_at: chrono::Local::now().format("%Y-%m-%d").to_string(),
            });
        }
    }

    /// Set a holding to exactly `shares` at `avg_cost` (no averaging), keeping
    /// its original `added_at`. Used to mirror a broker or import, so running
    /// the sync twice never doubles a position.
    pub fn set(&mut self, symbol: &str, shares: f64, avg_cost: f64) {
        let symbol_upper = symbol.to_uppercase();
        if let Some(h) = self.holdings.iter_mut().find(|h| h.symbol == symbol_upper) {
            h.shares = shares;
            h.avg_cost = avg_cost;
        } else {
            self.add(&symbol_upper, shares, avg_cost);
        }
    }

    pub fn remove(&mut self, symbol: &str) -> bool {
        let before = self.holdings.len();
        self.holdings.retain(|h| h.symbol != symbol.to_uppercase());
        self.holdings.len() < before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_new_holding() {
        let mut p = Portfolio::default();
        p.add("RELIANCE", 10.0, 2500.0);
        assert_eq!(p.holdings.len(), 1);
        assert_eq!(p.holdings[0].symbol, "RELIANCE");
        assert_eq!(p.holdings[0].shares, 10.0);
        assert_eq!(p.holdings[0].avg_cost, 2500.0);
    }

    #[test]
    fn test_add_uppercases_symbol() {
        let mut p = Portfolio::default();
        p.add("reliance", 5.0, 1000.0);
        assert_eq!(p.holdings[0].symbol, "RELIANCE");
    }

    #[test]
    fn test_add_averages_cost() {
        let mut p = Portfolio::default();
        p.add("RELIANCE", 10.0, 2000.0);
        p.add("RELIANCE", 10.0, 3000.0);
        assert_eq!(p.holdings.len(), 1);
        assert_eq!(p.holdings[0].shares, 20.0);
        assert!((p.holdings[0].avg_cost - 2500.0).abs() < 1e-9);
    }

    #[test]
    fn test_add_averages_cost_unequal_lots() {
        let mut p = Portfolio::default();
        p.add("TCS", 10.0, 3000.0); // 10 shares at 3000 = 30000
        p.add("TCS", 5.0, 3600.0); // 5 shares at 3600 = 18000
        // total cost 48000, total shares 15 → avg = 3200
        assert_eq!(p.holdings[0].shares, 15.0);
        assert!((p.holdings[0].avg_cost - 3200.0).abs() < 1e-9);
    }

    #[test]
    fn test_add_multiple_stocks() {
        let mut p = Portfolio::default();
        p.add("RELIANCE", 10.0, 2500.0);
        p.add("TCS", 5.0, 3500.0);
        p.add("INFY", 20.0, 1500.0);
        assert_eq!(p.holdings.len(), 3);
    }

    #[test]
    fn test_remove_existing() {
        let mut p = Portfolio::default();
        p.add("RELIANCE", 10.0, 2500.0);
        p.add("TCS", 5.0, 3500.0);
        assert!(p.remove("RELIANCE"));
        assert_eq!(p.holdings.len(), 1);
        assert_eq!(p.holdings[0].symbol, "TCS");
    }

    #[test]
    fn test_remove_nonexistent() {
        let mut p = Portfolio::default();
        p.add("RELIANCE", 10.0, 2500.0);
        assert!(!p.remove("TCS"));
        assert_eq!(p.holdings.len(), 1);
    }

    #[test]
    fn test_remove_case_insensitive() {
        let mut p = Portfolio::default();
        p.add("RELIANCE", 10.0, 2500.0);
        assert!(p.remove("reliance"));
        assert!(p.holdings.is_empty());
    }

    #[test]
    fn test_remove_from_empty() {
        let mut p = Portfolio::default();
        assert!(!p.remove("RELIANCE"));
    }

    #[test]
    fn test_holding_has_date() {
        let mut p = Portfolio::default();
        p.add("RELIANCE", 10.0, 2500.0);
        assert!(!p.holdings[0].added_at.is_empty());
    }
}
