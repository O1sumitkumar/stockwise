//! Capital-gains tax estimates: holding period per lot, tax years, and each
//! country's rules for setting losses off against gains. These are estimates
//! for listed shares only; `stockwise tax` labels them as such.

use chrono::{Datelike, Months, NaiveDate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Country {
    India,
    Us,
    Uk,
}

impl Country {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "in" | "india" => Some(Self::India),
            "us" | "usa" => Some(Self::Us),
            "uk" => Some(Self::Uk),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::India => "India",
            Self::Us => "United States",
            Self::Uk => "United Kingdom",
        }
    }

    pub fn currency_symbol(self) -> &'static str {
        match self {
            Self::India => "₹",
            Self::Us => "$",
            Self::Uk => "£",
        }
    }

    /// Months a share must be held for a gain to be long-term; `None` where
    /// the holding period doesn't matter (UK).
    pub fn long_term_months(self) -> Option<u32> {
        match self {
            Self::India | Self::Us => Some(12),
            Self::Uk => None,
        }
    }

    /// Short-term rate in percent: India Sec. 111A; US top ordinary-income
    /// bracket; UK higher-rate CGT (18% for basic-rate taxpayers).
    pub fn st_rate(self) -> f64 {
        match self {
            Self::India => 20.0,
            Self::Us => 37.0,
            Self::Uk => 24.0,
        }
    }

    /// Long-term rate in percent: India Sec. 112A; US top LTCG bracket.
    pub fn lt_rate(self) -> f64 {
        match self {
            Self::India => 12.5,
            Self::Us => 20.0,
            Self::Uk => 24.0,
        }
    }

    /// Yearly tax-free gains: India ₹1.25 lakh of LTCG; UK £3,000 annual exempt amount.
    pub fn exemption(self) -> f64 {
        match self {
            Self::India => 125_000.0,
            Self::Us => 0.0,
            Self::Uk => 3_000.0,
        }
    }

    /// Scale from a quote currency to this country's tax currency, or `None`
    /// for holdings in another currency (taxed under other rules).
    pub fn currency_scale(self, currency: &str) -> Option<f64> {
        match (self, currency) {
            (Self::India, "INR") | (Self::Us, "USD") | (Self::Uk, "GBP") => Some(1.0),
            (Self::Uk, "GBp") => Some(0.01),
            _ => None,
        }
    }

    pub fn tax_currency(self) -> &'static str {
        match self {
            Self::India => "INR",
            Self::Us => "USD",
            Self::Uk => "GBP",
        }
    }

    /// First and last day of the tax year containing `today`. India runs
    /// April–March; the UK year starts on 6 April; the US uses the calendar year.
    pub fn tax_year(self, today: NaiveDate) -> (NaiveDate, NaiveDate) {
        let y = today.year();
        let ymd = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).expect("valid date");
        match self {
            Self::Us => (ymd(y, 1, 1), ymd(y, 12, 31)),
            Self::India => {
                let start = if today.month() >= 4 { y } else { y - 1 };
                (ymd(start, 4, 1), ymd(start + 1, 3, 31))
            }
            Self::Uk => {
                let start = if (today.month(), today.day()) >= (4, 6) {
                    y
                } else {
                    y - 1
                };
                (ymd(start, 4, 6), ymd(start + 1, 4, 5))
            }
        }
    }

    /// "FY 2026-27" (India), "Tax year 2026-27" (UK), "Tax year 2026" (US).
    pub fn tax_year_label(self, today: NaiveDate) -> String {
        let (start, _) = self.tax_year(today);
        let y = start.year();
        match self {
            Self::Us => format!("Tax year {}", y),
            Self::Uk => format!("Tax year {}-{:02}", y, (y + 1) % 100),
            Self::India => format!("FY {}-{:02}", y, (y + 1) % 100),
        }
    }

    /// Whether shares bought on `bought` and sold on `sold` are a long-term holding.
    pub fn is_long_term(self, bought: NaiveDate, sold: NaiveDate) -> bool {
        self.long_term_months()
            .is_some_and(|m| is_long_term(bought, sold, m))
    }
}

/// Long-term when held for more than `months`: sold after the same calendar
/// date `months` later.
pub fn is_long_term(bought: NaiveDate, sold: NaiveDate, months: u32) -> bool {
    bought
        .checked_add_months(Months::new(months))
        .is_some_and(|threshold| sold > threshold)
}

/// Gains and losses split by holding period. All amounts are positive.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Buckets {
    pub st_gain: f64,
    pub st_loss: f64,
    pub lt_gain: f64,
    pub lt_loss: f64,
}

impl Buckets {
    pub fn add(&mut self, gain: f64, long_term: bool) {
        match (gain >= 0.0, long_term) {
            (true, true) => self.lt_gain += gain,
            (true, false) => self.st_gain += gain,
            (false, true) => self.lt_loss -= gain,
            (false, false) => self.st_loss -= gain,
        }
    }

    pub fn plus(self, o: Self) -> Self {
        Self {
            st_gain: self.st_gain + o.st_gain,
            st_loss: self.st_loss + o.st_loss,
            lt_gain: self.lt_gain + o.lt_gain,
            lt_loss: self.lt_loss + o.lt_loss,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.st_gain + self.st_loss + self.lt_gain + self.lt_loss == 0.0
    }
}

/// Tax on a set of gains after set-off. For the UK, which has no short/long
/// split, everything is reported in the short-term fields.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Estimate {
    /// Short-term gain left after set-off (taxable).
    pub st_taxable: f64,
    /// Long-term gain left after set-off, before the exemption.
    pub lt_net: f64,
    pub exemption_used: f64,
    pub lt_taxable: f64,
    pub st_tax: f64,
    pub lt_tax: f64,
    /// Short-term loss set off against long-term gains.
    pub st_loss_against_lt: f64,
    /// Long-term loss set off against short-term gains (US only).
    pub lt_loss_against_st: f64,
    /// Losses left over to carry forward.
    pub st_carry: f64,
    pub lt_carry: f64,
}

impl Estimate {
    pub fn total(&self) -> f64 {
        self.st_tax + self.lt_tax
    }
}

pub fn estimate(country: Country, b: Buckets) -> Estimate {
    match country {
        Country::India => india(b),
        Country::Us => us(b),
        Country::Uk => uk(b),
    }
}

/// India (listed shares with STT, from 23 July 2024): gains and losses of
/// the same kind net first. A net short-term loss is set off against
/// short-term gains and then long-term gains; a long-term loss only against
/// long-term gains — never short-term. Then ₹1.25 lakh of long-term gain is
/// exempt; STCG is taxed at 20%, the rest of LTCG at 12.5%. Unused losses
/// carry forward (8 years).
fn india(b: Buckets) -> Estimate {
    let c = Country::India;
    let mut st = b.st_gain - b.st_loss;
    let mut lt = b.lt_gain - b.lt_loss;
    let mut e = Estimate::default();
    if lt < 0.0 {
        e.lt_carry = -lt;
        lt = 0.0;
    }
    if st < 0.0 {
        let used = (-st).min(lt);
        e.st_loss_against_lt = used;
        e.st_carry = -st - used;
        lt -= used;
        st = 0.0;
    }
    e.st_taxable = st;
    e.lt_net = lt;
    e.exemption_used = lt.min(c.exemption());
    e.lt_taxable = lt - e.exemption_used;
    e.st_tax = e.st_taxable * c.st_rate() / 100.0;
    e.lt_tax = e.lt_taxable * c.lt_rate() / 100.0;
    e
}

/// US: short- and long-term results net within themselves, then a net loss
/// on one side offsets a net gain on the other. A loss left over is
/// deductible up to $3,000 a year against other income, the rest carries forward.
fn us(b: Buckets) -> Estimate {
    let c = Country::Us;
    let mut st = b.st_gain - b.st_loss;
    let mut lt = b.lt_gain - b.lt_loss;
    let mut e = Estimate::default();
    if st < 0.0 && lt > 0.0 {
        let used = (-st).min(lt);
        e.st_loss_against_lt = used;
        lt -= used;
        st += used;
    } else if lt < 0.0 && st > 0.0 {
        let used = (-lt).min(st);
        e.lt_loss_against_st = used;
        st -= used;
        lt += used;
    }
    e.st_carry = (-st).max(0.0);
    e.lt_carry = (-lt).max(0.0);
    e.st_taxable = st.max(0.0);
    e.lt_net = lt.max(0.0);
    e.lt_taxable = e.lt_net;
    e.st_tax = e.st_taxable * c.st_rate() / 100.0;
    e.lt_tax = e.lt_taxable * c.lt_rate() / 100.0;
    e
}

/// UK: no holding-period split. All gains and losses net, then £3,000 is
/// exempt and the rest is taxed at the CGT rate.
fn uk(b: Buckets) -> Estimate {
    let c = Country::Uk;
    let net = b.st_gain + b.lt_gain - b.st_loss - b.lt_loss;
    let mut e = Estimate::default();
    if net < 0.0 {
        e.st_carry = -net;
        return e;
    }
    e.exemption_used = net.min(c.exemption());
    e.st_taxable = net - e.exemption_used;
    e.st_tax = e.st_taxable * c.st_rate() / 100.0;
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn b(st_gain: f64, st_loss: f64, lt_gain: f64, lt_loss: f64) -> Buckets {
        Buckets {
            st_gain,
            st_loss,
            lt_gain,
            lt_loss,
        }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn long_term_means_held_more_than_twelve_months() {
        let bought = d("2024-03-15");
        assert!(!is_long_term(bought, d("2025-03-15"), 12));
        assert!(is_long_term(bought, d("2025-03-16"), 12));
        // Month ends clamp: 29 Feb + 12 months = 28 Feb.
        assert!(is_long_term(d("2024-02-29"), d("2025-03-01"), 12));
        assert!(!Country::Uk.is_long_term(bought, d("2030-01-01")));
    }

    #[test]
    fn tax_years() {
        let fy = Country::India.tax_year(d("2026-10-21"));
        assert_eq!(fy, (d("2026-04-01"), d("2027-03-31")));
        assert_eq!(Country::India.tax_year(d("2027-03-31")).0, d("2026-04-01"));
        assert_eq!(Country::Uk.tax_year(d("2026-04-05")).0, d("2025-04-06"));
        assert_eq!(Country::Uk.tax_year(d("2026-04-06")).0, d("2026-04-06"));
        assert_eq!(Country::Us.tax_year(d("2026-10-21")).1, d("2026-12-31"));
        assert_eq!(Country::India.tax_year_label(d("2026-10-21")), "FY 2026-27");
        assert_eq!(Country::India.tax_year_label(d("2027-01-05")), "FY 2026-27");
        assert_eq!(
            Country::Uk.tax_year_label(d("2026-04-05")),
            "Tax year 2025-26"
        );
        assert_eq!(Country::Us.tax_year_label(d("2026-10-21")), "Tax year 2026");
    }

    #[test]
    fn buckets_split_gains_and_losses() {
        let mut x = Buckets::default();
        x.add(100.0, false);
        x.add(-40.0, false);
        x.add(300.0, true);
        x.add(-10.0, true);
        assert_eq!(x, b(100.0, 40.0, 300.0, 10.0));
        assert_eq!(x.plus(x), b(200.0, 80.0, 600.0, 20.0));
    }

    // India worked examples (amounts in ₹).

    #[test]
    fn india_gains_only_exemption_applies_to_ltcg() {
        // STCG 1,00,000 → 20% = 20,000.
        // LTCG 2,00,000 − 1,25,000 exempt = 75,000 → 12.5% = 9,375.
        let e = estimate(Country::India, b(100_000.0, 0.0, 200_000.0, 0.0));
        assert!(close(e.st_tax, 20_000.0));
        assert!(close(e.exemption_used, 125_000.0));
        assert!(close(e.lt_taxable, 75_000.0));
        assert!(close(e.lt_tax, 9_375.0));
        assert!(close(e.total(), 29_375.0));
    }

    #[test]
    fn india_short_term_loss_nets_against_short_term_gain_first() {
        // STCG 1,00,000 − STCL 30,000 = 70,000 → 14,000. LTCG untouched.
        let e = estimate(Country::India, b(100_000.0, 30_000.0, 150_000.0, 0.0));
        assert!(close(e.st_taxable, 70_000.0));
        assert!(close(e.st_tax, 14_000.0));
        assert!(close(e.st_loss_against_lt, 0.0));
        assert!(close(e.lt_taxable, 25_000.0));
    }

    #[test]
    fn india_excess_short_term_loss_is_set_off_against_ltcg() {
        // STCL 50,000 with no STCG reduces LTCG 3,00,000 to 2,50,000;
        // 1,25,000 exempt, 1,25,000 taxed at 12.5% = 15,625.
        let e = estimate(Country::India, b(0.0, 50_000.0, 300_000.0, 0.0));
        assert!(close(e.st_loss_against_lt, 50_000.0));
        assert!(close(e.lt_net, 250_000.0));
        assert!(close(e.lt_tax, 15_625.0));
        assert!(close(e.st_carry, 0.0));
    }

    #[test]
    fn india_long_term_loss_never_reduces_stcg() {
        // LTCL 1,00,000 can't touch STCG 1,00,000: 20,000 tax, LTCL carried.
        let e = estimate(Country::India, b(100_000.0, 0.0, 0.0, 100_000.0));
        assert!(close(e.st_tax, 20_000.0));
        assert!(close(e.lt_carry, 100_000.0));
        assert!(close(e.total(), 20_000.0));
    }

    #[test]
    fn india_mixed_losses_with_carry_forward() {
        // STCG 40k − STCL 1L = −60k short-term. LTCG 50k − LTCL 20k = 30k.
        // 30k of the short-term loss wipes out LTCG; 30k carries forward.
        let e = estimate(Country::India, b(40_000.0, 100_000.0, 50_000.0, 20_000.0));
        assert!(close(e.st_loss_against_lt, 30_000.0));
        assert!(close(e.st_carry, 30_000.0));
        assert!(close(e.lt_carry, 0.0));
        assert!(close(e.total(), 0.0));
    }

    #[test]
    fn india_ltcg_within_the_exemption_is_tax_free() {
        let e = estimate(Country::India, b(0.0, 0.0, 90_000.0, 0.0));
        assert!(close(e.exemption_used, 90_000.0));
        assert!(close(e.total(), 0.0));
    }

    #[test]
    fn us_net_loss_on_one_side_offsets_the_other() {
        // ST −5,000, LT +20,000 → LT 15,000 at 20% = 3,000.
        let e = estimate(Country::Us, b(1_000.0, 6_000.0, 20_000.0, 0.0));
        assert!(close(e.st_loss_against_lt, 5_000.0));
        assert!(close(e.lt_tax, 3_000.0));
        // LT −4,000 against ST +10,000 → ST 6,000 at 37%.
        let e = estimate(Country::Us, b(10_000.0, 0.0, 0.0, 4_000.0));
        assert!(close(e.lt_loss_against_st, 4_000.0));
        assert!(close(e.st_tax, 6_000.0 * 0.37));
        // Both negative: everything carries.
        let e = estimate(Country::Us, b(0.0, 1.0, 0.0, 2.0));
        assert!(close(e.st_carry, 1.0) && close(e.lt_carry, 2.0));
    }

    #[test]
    fn uk_nets_everything_then_applies_the_annual_exemption() {
        // Gains 10,000 − losses 2,000 = 8,000; 3,000 exempt; 5,000 at 24%.
        let e = estimate(Country::Uk, b(6_000.0, 2_000.0, 4_000.0, 0.0));
        assert!(close(e.exemption_used, 3_000.0));
        assert!(close(e.total(), 1_200.0));
        let e = estimate(Country::Uk, b(0.0, 500.0, 0.0, 0.0));
        assert!(close(e.st_carry, 500.0));
    }

    #[test]
    fn currency_scale_picks_the_tax_currency() {
        assert_eq!(Country::India.currency_scale("INR"), Some(1.0));
        assert_eq!(Country::India.currency_scale("USD"), None);
        assert_eq!(Country::Uk.currency_scale("GBp"), Some(0.01));
        assert_eq!(Country::parse("India"), Some(Country::India));
        assert_eq!(Country::parse("fr"), None);
    }
}
