use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Watchlist {
    pub symbols: Vec<String>,
}

impl Watchlist {
    pub fn load() -> Result<Self> {
        crate::store::load("watchlist.json")
    }

    pub fn save(&self) -> Result<()> {
        crate::store::save("watchlist.json", self, false)
    }

    pub fn add(&mut self, symbol: &str) -> bool {
        let upper = symbol.to_uppercase();
        if self.symbols.contains(&upper) {
            return false;
        }
        self.symbols.push(upper);
        true
    }

    pub fn remove(&mut self, symbol: &str) -> bool {
        let upper = symbol.to_uppercase();
        let before = self.symbols.len();
        self.symbols.retain(|s| s != &upper);
        self.symbols.len() < before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_new_symbol() {
        let mut wl = Watchlist::default();
        assert!(wl.add("RELIANCE"));
        assert_eq!(wl.symbols, vec!["RELIANCE"]);
    }

    #[test]
    fn test_add_uppercases_symbol() {
        let mut wl = Watchlist::default();
        wl.add("reliance");
        assert_eq!(wl.symbols[0], "RELIANCE");
    }

    #[test]
    fn test_add_duplicate_returns_false() {
        let mut wl = Watchlist::default();
        assert!(wl.add("RELIANCE"));
        assert!(!wl.add("RELIANCE"));
        assert_eq!(wl.symbols.len(), 1);
    }

    #[test]
    fn test_add_duplicate_case_insensitive() {
        let mut wl = Watchlist::default();
        assert!(wl.add("reliance"));
        assert!(!wl.add("RELIANCE"));
        assert_eq!(wl.symbols.len(), 1);
    }

    #[test]
    fn test_add_multiple_symbols() {
        let mut wl = Watchlist::default();
        wl.add("RELIANCE");
        wl.add("TCS");
        wl.add("INFY");
        assert_eq!(wl.symbols.len(), 3);
    }

    #[test]
    fn test_remove_existing_symbol() {
        let mut wl = Watchlist::default();
        wl.add("RELIANCE");
        wl.add("TCS");
        assert!(wl.remove("RELIANCE"));
        assert_eq!(wl.symbols.len(), 1);
        assert_eq!(wl.symbols[0], "TCS");
    }

    #[test]
    fn test_remove_nonexistent_returns_false() {
        let mut wl = Watchlist::default();
        wl.add("RELIANCE");
        assert!(!wl.remove("TCS"));
        assert_eq!(wl.symbols.len(), 1);
    }

    #[test]
    fn test_remove_case_insensitive() {
        let mut wl = Watchlist::default();
        wl.add("RELIANCE");
        assert!(wl.remove("reliance"));
        assert!(wl.symbols.is_empty());
    }

    #[test]
    fn test_remove_from_empty() {
        let mut wl = Watchlist::default();
        assert!(!wl.remove("RELIANCE"));
    }
}
