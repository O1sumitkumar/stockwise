//! Shape tests for the pure builders behind `--json` and `--csv`. The key
//! lists here are the documented shapes in docs/commands.md: a failing test
//! means the docs (and anyone's scripts) need to change too.

use super::fundamentals::{CompareRow, dividend_json};
use super::holdings::{alert_check_rows, holding_rows, tax_report, watch_rows};
use super::markets::{MoverRow, mover_rows};
use super::overview::QuoteRow;
use super::risk::{correlation_csv, correlation_matrix, return_row, risk_json};
use super::sim::sim_history_json;
use super::strategies::{backtest_json, trade_rows};
use super::technicals::technical_json;
use super::*;
use crate::api::Quote;
use crate::output::{missing_columns, to_csv};
use crate::portfolio::Holding;

fn quote(symbol: &str, price: Option<f64>, change_pct: Option<f64>) -> Quote {
    Quote {
        symbol: Some(symbol.into()),
        regular_market_price: price,
        regular_market_change_percent: change_pct,
        currency: Some("INR".into()),
        ..Default::default()
    }
}

fn holding(symbol: &str, shares: f64, avg_cost: f64, added_at: &str) -> Holding {
    Holding {
        symbol: symbol.into(),
        shares,
        avg_cost,
        added_at: added_at.into(),
    }
}

fn keys(v: &serde_json::Value) -> Vec<&str> {
    v.as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect()
}

fn assert_columns<R: CsvRow>(row: &R) {
    let (missing, extra) = missing_columns(row);
    assert!(
        missing.is_empty() && extra.is_empty(),
        "{}: columns not in the row {:?}, fields not in the columns {:?}",
        std::any::type_name::<R>(),
        missing,
        extra
    );
}

/// A rising-then-falling series long enough for every indicator.
fn closes(n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| 100.0 + (i as f64 / 7.0).sin() * 10.0 + i as f64 * 0.1)
        .collect()
}

#[test]
fn portfolio_totals_skip_holdings_without_a_price() {
    let holdings = [
        holding("TCS.NS", 10.0, 3000.0, "2025-01-02"),
        holding("GONE.NS", 5.0, 100.0, "2025-01-02"),
    ];
    let quotes = [quote("TCS.NS", Some(3300.0), Some(1.0))];
    let (rows, totals) = holding_rows(&holdings, &quotes);

    assert_eq!(rows.len(), 2, "unpriced holdings stay in the rows");
    assert_eq!(rows[0].value, Some(33000.0));
    assert_eq!(rows[0].pnl, Some(3000.0));
    assert_eq!(rows[0].pnl_percent, Some(10.0));
    assert_eq!(rows[0].weight_percent, Some(100.0));
    assert_eq!(rows[1].price, None);
    assert_eq!(rows[1].value, None, "a missing price is never valued at 0");
    assert_eq!(totals.cost_basis, 30000.0);
    assert_eq!(totals.value, 33000.0);
    assert_eq!(totals.pnl_percent, 10.0);

    let row = serde_json::to_value(&rows[0]).unwrap();
    assert_eq!(
        keys(&row),
        [
            "symbol",
            "shares",
            "avg_cost",
            "cost_basis",
            "price",
            "value",
            "pnl",
            "pnl_percent",
            "weight_percent",
            "currency"
        ]
    );
    assert_eq!(
        keys(&serde_json::to_value(&totals).unwrap()),
        ["cost_basis", "value", "pnl", "pnl_percent", "currency"]
    );
    assert_columns(&rows[0]);
}

#[test]
fn portfolio_csv_leaves_missing_prices_empty() {
    let holdings = [holding("GONE.NS", 5.0, 100.0, "2025-01-02")];
    let (rows, _) = holding_rows(&holdings, &[]);
    assert_eq!(
        to_csv(&rows).unwrap().lines().nth(1),
        Some("GONE.NS,5.0,100.0,500.0,,,,,,")
    );
}

#[test]
fn watch_rows_follow_the_watchlist_order() {
    let rows = watch_rows(
        &["INFY.NS".into(), "GONE.NS".into(), "TCS.NS".into()],
        &[
            quote("TCS.NS", Some(1.0), Some(0.5)),
            quote("INFY.NS", Some(2.0), None),
        ],
    );
    let syms: Vec<&str> = rows.iter().map(|r| r.symbol.as_str()).collect();
    assert_eq!(syms, ["INFY.NS", "GONE.NS", "TCS.NS"]);
    assert_eq!(rows[1].price, None);
    assert_columns(&rows[0]);
}

#[test]
fn alert_check_never_triggers_without_a_price() {
    let mut store = AlertStore::default();
    store.add("TCS.NS", AlertCondition::Above, 3000.0);
    store.add("INFY.NS", AlertCondition::Below, 1000.0);
    store.add("GONE.NS", AlertCondition::Below, 1e9);
    let rows = alert_check_rows(
        &store.alerts,
        &[
            quote("TCS.NS", Some(3100.0), None),
            quote("INFY.NS", Some(1500.0), None),
        ],
    );
    let status: Vec<&str> = rows.iter().map(|r| r.status).collect();
    assert_eq!(status, ["triggered", "waiting", "no_price"]);
    assert_eq!(rows[2].index, 2);
    assert_columns(&rows[0]);
}

#[test]
fn tax_report_splits_terms_and_skips_unpriced() {
    let today = chrono::NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
    let holdings = [
        holding("OLD.NS", 10.0, 100.0, "2024-01-01"),
        holding("NEW.NS", 10.0, 100.0, "2026-05-01"),
        holding("GONE.NS", 10.0, 100.0, "2026-05-01"),
    ];
    let quotes = [
        quote("OLD.NS", Some(150.0), None),
        quote("NEW.NS", Some(80.0), None),
    ];
    let (rows, sum, unpriced) = tax_report("in", &holdings, &quotes, today).unwrap();
    assert_eq!(unpriced, ["GONE.NS"]);
    assert_eq!(rows[0].term, "long");
    assert_eq!(rows[0].gain, 500.0);
    assert_eq!(rows[1].term, "short");
    assert_eq!(rows[1].gain, -200.0);
    assert_eq!(rows[1].tax, 0.0);
    assert_eq!(sum.long_term_gains, 500.0);
    assert_eq!(sum.short_term_losses, 200.0);
    // ₹500 LTCG is under the ₹1.25L exemption.
    assert_eq!(sum.total_tax, 0.0);
    assert!(tax_report("fr", &holdings, &quotes, today).is_err());

    assert_eq!(
        keys(&serde_json::to_value(&sum).unwrap()),
        [
            "short_term_gains",
            "short_term_losses",
            "long_term_gains",
            "long_term_losses",
            "stcg_rate_percent",
            "ltcg_rate_percent",
            "ltcg_exemption",
            "stcg_tax",
            "ltcg_tax",
            "total_tax",
            "tax_saved_by_losses"
        ]
    );
    assert_columns(&rows[0]);
}

#[test]
fn movers_leave_out_quotes_without_a_change() {
    let quotes = [
        quote("A", Some(1.0), Some(3.0)),
        quote("B", Some(1.0), None),
        quote("C", Some(1.0), Some(-2.0)),
        quote("D", None, Some(9.0)),
        quote("E", Some(1.0), Some(0.5)),
    ];
    let (gainers, losers) = mover_rows(&quotes, 2);
    let g: Vec<&str> = gainers.iter().map(|r| r.symbol.as_str()).collect();
    let l: Vec<&str> = losers.iter().map(|r| r.symbol.as_str()).collect();
    assert_eq!(g, ["A", "E"]);
    assert_eq!(l, ["C", "E"]);
    assert_eq!((gainers[0].side, gainers[0].rank), ("gainer", 1));
    assert_eq!((losers[0].side, losers[0].rank), ("loser", 1));
    assert_columns::<MoverRow>(&gainers[0]);
}

#[test]
fn return_row_needs_two_closes() {
    assert!(return_row("5d", "1 Week", &[100.0]).is_none());
    let r = return_row("1mo", "1 Month", &[100.0, 105.0, 110.0]).unwrap();
    assert_eq!(r.return_percent, 10.0);
    assert_eq!(r.growth_of_100000, 110000.0);
    assert_columns(&r);
}

#[test]
fn correlation_matrix_has_a_unit_diagonal_and_nulls_for_gaps() {
    let a = closes(60);
    let series = vec![
        ("A".to_string(), a.clone()),
        ("B".to_string(), a.iter().map(|x| x * 2.0).collect()),
        ("EMPTY".to_string(), Vec::new()),
    ];
    let m = correlation_matrix(&series);
    assert_eq!(m[0][0], Some(1.0));
    assert_eq!(m[0][1], Some(1.0));
    assert_eq!(m[0][2], None);
    assert_eq!(
        correlation_csv(&["A", "B", "EMPTY"], &m),
        "symbol,A,B,EMPTY\nA,1.0,1.0,\nB,1.0,1.0,\nEMPTY,,,1.0\n"
    );
}

#[test]
fn technical_json_has_the_documented_keys() {
    let c = closes(250);
    let v = technical_json("TCS.NS", Some("INR"), "1y", "1d", &c, &c, &c, &[1000; 250]);
    assert_eq!(
        keys(&v),
        [
            "symbol",
            "currency",
            "period",
            "interval",
            "bars",
            "price",
            "sma_10",
            "sma_20",
            "sma_50",
            "sma_100",
            "sma_200",
            "ema_12",
            "ema_26",
            "rsi_14",
            "rsi_signal",
            "macd",
            "bollinger",
            "atr_14",
            "atr_percent",
            "vwap",
            "bias"
        ]
    );
    assert_eq!(keys(&v["macd"]), ["line", "signal", "histogram", "trend"]);
    assert_eq!(
        keys(&v["bollinger"]),
        ["upper", "middle", "lower", "position"]
    );
    assert_eq!(keys(&v["bias"]), ["verdict", "bullish", "bearish"]);
    assert!(v["sma_200"].is_number());

    // Too few bars for the long averages: null, not 0.
    let short = closes(40);
    let v = technical_json("X", None, "3mo", "1d", &short, &short, &short, &[1; 40]);
    assert!(v["sma_200"].is_null());
}

#[test]
fn risk_json_has_the_documented_keys() {
    let v = risk_json("TCS.NS", Some("INR"), "1y", &closes(250));
    assert_eq!(
        keys(&v),
        [
            "symbol",
            "currency",
            "period",
            "interval",
            "bars",
            "price",
            "return_percent",
            "volatility_percent",
            "risk_level",
            "sharpe",
            "sortino",
            "calmar",
            "max_drawdown",
            "var_95_percent",
            "var_99_percent"
        ]
    );
    assert_eq!(
        keys(&v["max_drawdown"]),
        ["percent", "peak_price", "trough_price", "trading_days"]
    );
}

#[test]
fn dividend_json_without_a_dividend_has_no_projections() {
    let v = dividend_json("X.NS", Some("INR"), 100.0, 0.0);
    assert_eq!(v["pays_dividend"], false);
    assert_eq!(v["income"].as_array().unwrap().len(), 0);

    let v = dividend_json("X.NS", Some("INR"), 100.0, 0.05);
    assert_eq!(
        keys(&v),
        [
            "symbol",
            "currency",
            "price",
            "pays_dividend",
            "dividend_yield_percent",
            "annual_dividend_per_share",
            "income",
            "drip"
        ]
    );
    assert_eq!(v["dividend_yield_percent"], 5.0);
    assert_eq!(v["income"][0]["annual_income"], 5000.0);
    let years: Vec<i64> = v["drip"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["year"].as_i64().unwrap())
        .collect();
    assert_eq!(years, [1, 5, 10]);
}

#[test]
fn backtest_trades_get_dates_from_their_bars() {
    let result = backtest::BacktestResult {
        strategy: "rsi".into(),
        trades: vec![backtest::Trade {
            entry_idx: 1,
            exit_idx: 2,
            entry_price: 100.0,
            exit_price: 110.0,
            pnl_pct: 10.0,
        }],
        total_return: 10.0,
        win_rate: 100.0,
        avg_win: 10.0,
        avg_loss: 0.0,
        max_drawdown: 0.0,
        sharpe: None,
        buy_hold_return: 5.0,
        equity_curve: vec![100.0, 110.0],
    };
    // 2024-01-01, 2024-01-02, 2024-01-03 00:00 UTC
    let times = [1704067200, 1704153600, 1704240000];
    let trades = trade_rows(&result, &times);
    assert_eq!(trades[0].entry_date.as_deref(), Some("2024-01-02"));
    assert_eq!(trades[0].exit_date.as_deref(), Some("2024-01-03"));
    assert_columns(&trades[0]);

    let v = backtest_json("X", None, "rsi", "2y", 3, &result, &trades);
    assert_eq!(v["alpha_percent"], 5.0);
    assert_eq!(v["trade_count"], 1);
    assert!(v["sharpe"].is_null());
    assert_eq!(v["trades"][0]["number"], 1);
}

#[test]
fn empty_sim_history_is_not_an_error() {
    let history = SimHistory::default();
    let v = sim_history_json(&history, &history.stats());
    assert_eq!(keys(&v), ["stats", "confidence", "sessions"]);
    assert_eq!(v["stats"]["days"], 0);
    assert!(v["confidence"].is_null());
    assert_eq!(v["sessions"].as_array().unwrap().len(), 0);
}

#[test]
fn quote_and_compare_rows_match_their_columns() {
    let q = quote("TCS.NS", Some(1.0), Some(1.0));
    assert_columns(&QuoteRow::from(&q));
    assert_columns(&CompareRow::from(&q));
}

#[test]
fn inline_rows_match_their_columns() {
    assert_columns(&crate::views::IndexRow {
        region: "Indian Markets".into(),
        symbol: "^NSEI".into(),
        name: "NIFTY 50".into(),
        price: None,
        change: None,
        change_percent: None,
        currency: None,
    });
    assert_columns(&super::overview::SearchRow {
        symbol: None,
        name: None,
        quote_type: None,
        exchange: None,
    });
    assert_columns(&super::markets::SectorRow {
        sector: "IT".into(),
        symbol: "^CNXIT".into(),
        price: None,
        change_percent: None,
        currency: None,
    });
    assert_columns(&super::markets::ScreenRow {
        symbol: "TCS.NS".into(),
        name: None,
        price: None,
        change_percent: None,
        pe: None,
        market_cap: None,
        dividend_yield: None,
        currency: None,
    });
    assert_columns(&super::holdings::AlertRow {
        index: 0,
        symbol: "TCS.NS".into(),
        condition: "above".into(),
        target: 1.0,
        created_at: "2026-01-01 09:15".into(),
    });
    assert_columns(&crate::views::Bar {
        time: None,
        open: None,
        high: None,
        low: None,
        close: None,
        volume: None,
        ts: 0,
    });
}
