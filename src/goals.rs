//! Goal planning math: the monthly SIP needed for a target corpus (with an
//! optional yearly step-up), a retirement corpus that pays inflation-linked
//! monthly expenses, and an education goal. Amounts can be typed the way
//! Indians write them (`1cr`, `25L`, `50k`). Pure functions; `stockwise goal`
//! prints them.
//!
//! Conventions, as Indian SIP calculators use them: the monthly rate is the
//! annual return ÷ 12, SIPs are paid at the start of each month, and a
//! step-up raises the SIP once a year.

use serde::Serialize;

/// Parse an amount such as `1cr`, `1.5 crore`, `25L`, `25 lakh`, `50k`,
/// `2m` (million), `₹2,50,000` or `250000`.
pub fn parse_amount(s: &str) -> Result<f64, String> {
    let cleaned: String = s
        .trim()
        .trim_start_matches(['₹', '$'])
        .chars()
        .filter(|c| !matches!(c, ',' | '_' | ' '))
        .collect::<String>()
        .to_lowercase();
    let split = cleaned
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(cleaned.len());
    let (number, unit) = cleaned.split_at(split);
    let scale = match unit {
        "" => 1.0,
        "k" | "thousand" => 1e3,
        "l" | "lakh" | "lakhs" | "lac" | "lacs" => 1e5,
        "m" | "mn" | "million" => 1e6,
        "cr" | "crore" | "crores" => 1e7,
        "b" | "bn" | "billion" => 1e9,
        _ => return Err(amount_error(s)),
    };
    match number.parse::<f64>() {
        Ok(n) if n.is_finite() && n > 0.0 && n * scale <= 1e15 => Ok(n * scale),
        _ => Err(amount_error(s)),
    }
}

fn amount_error(s: &str) -> String {
    format!("`{s}` is not an amount: use e.g. 1cr, 25L, 50k or 250000")
}

/// Monthly rate for an annual return in percent: annual ÷ 12.
pub fn monthly_rate(annual_pct: f64) -> f64 {
    annual_pct / 100.0 / 12.0
}

/// What `n` monthly payments of 1, each at the start of a month, grow to by
/// the end of the last month at monthly rate `i`.
fn annuity_due_fv(i: f64, n: u32) -> f64 {
    if i.abs() < 1e-12 {
        n as f64
    } else {
        ((1.0 + i).powi(n as i32) - 1.0) / i * (1.0 + i)
    }
}

/// What a monthly SIP of 1 in its first year grows to after `years`, when
/// the SIP rises by `step_up_pct` at the start of every year.
pub fn sip_growth_factor(years: u32, annual_return_pct: f64, step_up_pct: f64) -> f64 {
    let i = monthly_rate(annual_return_pct);
    let year = annuity_due_fv(i, 12);
    let g = 1.0 + step_up_pct / 100.0;
    (0..years)
        .map(|y| g.powi(y as i32) * year * (1.0 + i).powi((12 * (years - 1 - y)) as i32))
        .sum()
}

/// One year of a SIP plan.
#[derive(Debug, Clone, Serialize)]
pub struct SipYear {
    pub year: u32,
    pub monthly_sip: f64,
    /// SIP amounts paid so far (not counting existing savings).
    pub invested: f64,
    /// Value at the end of the year, existing savings included.
    pub value: f64,
}

/// The monthly SIP that reaches `target` after `years`.
#[derive(Debug, Clone, Serialize)]
pub struct SipPlan {
    pub target: f64,
    pub years: u32,
    pub annual_return_pct: f64,
    pub step_up_pct: f64,
    /// Money already invested towards the goal; it grows at the same return.
    pub savings: f64,
    /// What `savings` grows to by the end.
    pub savings_future_value: f64,
    /// SIP in the first year (0 when savings already cover the target).
    pub monthly_sip: f64,
    /// SIP in the last year, after every step-up.
    pub final_monthly_sip: f64,
    /// Total of all SIP payments.
    pub total_invested: f64,
    /// Final value minus everything paid in (savings and SIPs).
    pub gains: f64,
    /// A single investment today that reaches the target instead.
    pub lump_sum_today: f64,
    pub schedule: Vec<SipYear>,
}

pub fn plan_sip(
    target: f64,
    years: u32,
    annual_return_pct: f64,
    step_up_pct: f64,
    savings: f64,
) -> SipPlan {
    let i = monthly_rate(annual_return_pct);
    let growth = (1.0 + i).powi((12 * years) as i32);
    let savings_future_value = savings * growth;
    let needed = (target - savings_future_value).max(0.0);
    let factor = sip_growth_factor(years, annual_return_pct, step_up_pct);
    let monthly_sip = if needed > 0.0 && factor > 0.0 {
        needed / factor
    } else {
        0.0
    };

    let g = 1.0 + step_up_pct / 100.0;
    let year_fv = annuity_due_fv(i, 12);
    let mut schedule = Vec::with_capacity(years as usize);
    let (mut value, mut invested) = (savings, 0.0);
    for y in 0..years {
        let sip = monthly_sip * g.powi(y as i32);
        invested += sip * 12.0;
        value = value * (1.0 + i).powi(12) + sip * year_fv;
        schedule.push(SipYear {
            year: y + 1,
            monthly_sip: sip,
            invested,
            value,
        });
    }
    let final_monthly_sip = schedule.last().map_or(0.0, |y| y.monthly_sip);
    SipPlan {
        target,
        years,
        annual_return_pct,
        step_up_pct,
        savings,
        savings_future_value,
        monthly_sip,
        final_monthly_sip,
        total_invested: invested,
        gains: value - invested - savings,
        lump_sum_today: target / growth,
        schedule,
    }
}

/// `amount` after `years` of inflation at `inflation_pct` a year.
pub fn inflate(amount: f64, inflation_pct: f64, years: u32) -> f64 {
    amount * (1.0 + inflation_pct / 100.0).powi(years as i32)
}

/// Corpus needed at retirement to pay `monthly_expense` (in money of the
/// first retirement year) at the start of every month for `years`, with the
/// expense rising by `inflation_pct` once a year while the rest earns
/// `return_pct` (monthly rate = annual ÷ 12). The corpus runs out exactly at
/// the end.
pub fn retirement_corpus(
    monthly_expense: f64,
    years: u32,
    inflation_pct: f64,
    return_pct: f64,
) -> f64 {
    let v = 1.0 / (1.0 + monthly_rate(return_pct));
    // Present value of 12 payments of 1 at the start of each month.
    let year: f64 = (0..12).map(|m| v.powi(m)).sum();
    // Each year's payments are `inflation` higher and 12 months further away.
    let q = (1.0 + inflation_pct / 100.0) * v.powi(12);
    let years_sum: f64 = (0..years).map(|y| q.powi(y as i32)).sum();
    monthly_expense * year * years_sum
}

#[derive(Debug, Clone, Serialize)]
pub struct RetirePlan {
    pub monthly_expense_today: f64,
    pub years_to_retire: u32,
    pub years_in_retirement: u32,
    pub inflation_pct: f64,
    pub return_pre_pct: f64,
    pub return_post_pct: f64,
    /// Today's monthly expense in money of the first retirement year.
    pub monthly_expense_at_retirement: f64,
    /// Corpus needed on the day you retire.
    pub corpus: f64,
    /// How to build `corpus` before retiring.
    pub sip: SipPlan,
}

pub fn plan_retirement(
    monthly_expense_today: f64,
    years_to_retire: u32,
    years_in_retirement: u32,
    inflation_pct: f64,
    return_pre_pct: f64,
    return_post_pct: f64,
    step_up_pct: f64,
    savings: f64,
) -> RetirePlan {
    let at_retirement = inflate(monthly_expense_today, inflation_pct, years_to_retire);
    let corpus = retirement_corpus(
        at_retirement,
        years_in_retirement,
        inflation_pct,
        return_post_pct,
    );
    RetirePlan {
        monthly_expense_today,
        years_to_retire,
        years_in_retirement,
        inflation_pct,
        return_pre_pct,
        return_post_pct,
        monthly_expense_at_retirement: at_retirement,
        corpus,
        sip: plan_sip(
            corpus,
            years_to_retire,
            return_pre_pct,
            step_up_pct,
            savings,
        ),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EducationPlan {
    pub cost_today: f64,
    pub years: u32,
    pub inflation_pct: f64,
    /// Today's cost after `years` of education inflation.
    pub future_cost: f64,
    pub sip: SipPlan,
}

pub fn plan_education(
    cost_today: f64,
    years: u32,
    inflation_pct: f64,
    return_pct: f64,
    step_up_pct: f64,
    savings: f64,
) -> EducationPlan {
    let future_cost = inflate(cost_today, inflation_pct, years);
    EducationPlan {
        cost_today,
        years,
        inflation_pct,
        future_cost,
        sip: plan_sip(future_cost, years, return_pct, step_up_pct, savings),
    }
}

/// Indian digit grouping: 12345678.4 → "1,23,45,678".
pub fn indian_grouping(amount: f64) -> String {
    let n = amount.abs().round() as u64;
    let digits = n.to_string();
    let grouped = if digits.len() <= 3 {
        digits
    } else {
        let (head, last3) = digits.split_at(digits.len() - 3);
        let mut parts: Vec<String> = Vec::new();
        let mut rest = head;
        while rest.len() > 2 {
            let (a, b) = rest.split_at(rest.len() - 2);
            parts.push(b.to_string());
            rest = a;
        }
        parts.push(rest.to_string());
        parts.reverse();
        format!("{},{}", parts.join(","), last3)
    };
    if amount < 0.0 && n > 0 {
        format!("-{grouped}")
    } else {
        grouped
    }
}

/// Thousands grouping: 1234567.4 → "1,234,567".
pub fn western_grouping(amount: f64) -> String {
    let n = amount.abs().round() as u64;
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if amount < 0.0 && n > 0 {
        format!("-{out}")
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn indian_amounts_parse() {
        assert_eq!(parse_amount("1cr"), Ok(1e7));
        assert_eq!(parse_amount("1.5 Crore"), Ok(1.5e7));
        assert_eq!(parse_amount("25L"), Ok(25e5));
        assert_eq!(parse_amount("25 lakh"), Ok(25e5));
        assert_eq!(parse_amount("7lacs"), Ok(7e5));
        assert_eq!(parse_amount("50k"), Ok(5e4));
        assert_eq!(parse_amount("2m"), Ok(2e6));
        assert_eq!(parse_amount("₹2,50,000"), Ok(250_000.0));
        assert_eq!(parse_amount("80000"), Ok(80_000.0));
        for bad in ["", "abc", "1x", "-5L", "0", "1.2.3cr", "cr"] {
            assert!(parse_amount(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn sip_for_one_crore_in_fifteen_years() {
        // i = 1% a month, n = 180: FV factor = ((1.01^180 − 1) / 0.01) × 1.01
        // = (5.995802 − 1) / 0.01 × 1.01 = 504.576; ₹1 Cr / 504.576 = ₹19,819.
        let p = plan_sip(1e7, 15, 12.0, 0.0, 0.0);
        assert!(near(p.monthly_sip, 19_818.6, 1.0), "{}", p.monthly_sip);
        assert!(near(p.total_invested, 19_818.6 * 180.0, 200.0));
        assert!(near(p.schedule.last().unwrap().value, 1e7, 1e-3));
        // Lump sum today: 1 Cr / 1.01^180 = ₹16.68 L.
        assert!(
            near(p.lump_sum_today, 1_667_833.0, 5.0),
            "{}",
            p.lump_sum_today
        );
        assert_eq!(p.final_monthly_sip, p.monthly_sip);
    }

    #[test]
    fn step_up_lowers_the_first_sip() {
        // 10% yearly step-up: factor = 12.80933 × Σ 1.1^y × 1.126825^(14−y)
        // = 12.80933 × 67.794 = 868.39, so ₹1 Cr needs ₹11,516 a month at first.
        let p = plan_sip(1e7, 15, 12.0, 10.0, 0.0);
        assert!(near(p.monthly_sip, 11_515.6, 1.0), "{}", p.monthly_sip);
        assert!(near(
            p.final_monthly_sip,
            p.monthly_sip * 1.1f64.powi(14),
            1e-6
        ));
        // Month by month, the stepped-up SIP lands on the target.
        let i = 0.01;
        let mut value = 0.0;
        for y in 0..15 {
            for _ in 0..12 {
                value = (value + p.monthly_sip * 1.1f64.powi(y)) * (1.0 + i);
            }
        }
        assert!(near(value, 1e7, 1e-3), "{value}");
    }

    #[test]
    fn zero_return_is_just_saving() {
        let p = plan_sip(1_200_000.0, 10, 0.0, 0.0, 0.0);
        assert!(near(p.monthly_sip, 10_000.0, 1e-9));
        assert!(near(p.gains, 0.0, 1e-6));
    }

    #[test]
    fn savings_reduce_or_cover_the_sip() {
        // ₹10 L saved grows to 10 L × 1.01^180 = ₹59.96 L of the ₹1 Cr.
        let p = plan_sip(1e7, 15, 12.0, 0.0, 1e6);
        assert!(near(p.savings_future_value, 5_995_802.0, 5.0));
        assert!(near(p.monthly_sip, (1e7 - 5_995_802.0) / 504.576, 1.0));
        let covered = plan_sip(1e6, 15, 12.0, 0.0, 1e6);
        assert_eq!(covered.monthly_sip, 0.0);
        assert!(covered.schedule.iter().all(|y| y.monthly_sip == 0.0));
    }

    #[test]
    fn retirement_corpus_matches_the_closed_form() {
        // ₹80,000 today at 6% for 20 years: 80,000 × 1.06^20 = ₹2,56,571 a month.
        let plan = plan_retirement(80_000.0, 20, 30, 6.0, 12.0, 7.0, 0.0, 0.0);
        assert!(near(plan.monthly_expense_at_retirement, 256_570.8, 0.5));
        // v = 1/1.0058333; a year of payments = (1 − v^12)/(1 − v) = 11.62454;
        // q = 1.06 × v^12 = 0.988538; Σ q^y (30 years) = 25.50867;
        // corpus = 2,56,570.84 × 11.62454 × 25.50867 = ₹7.608 Cr.
        assert!(near(plan.corpus, 76_080_039.0, 50.0), "{}", plan.corpus);
        // Drawing the expense month by month (rising 6% a year) empties it.
        let i = 0.07 / 12.0;
        let mut left = plan.corpus;
        for y in 0..30 {
            for _ in 0..12 {
                left = (left - plan.monthly_expense_at_retirement * 1.06f64.powi(y)) * (1.0 + i);
            }
        }
        assert!(left.abs() < 1.0, "{left}");
        // And the SIP builds exactly that corpus in 20 years at 12%.
        assert!(near(
            plan.sip.schedule.last().unwrap().value,
            plan.corpus,
            1e-2
        ));
    }

    #[test]
    fn retirement_when_return_equals_inflation_is_expense_times_months() {
        // With a 0% return and 0% inflation the corpus is every payment summed.
        let c = retirement_corpus(50_000.0, 25, 0.0, 0.0);
        assert!(near(c, 50_000.0 * 12.0 * 25.0, 1e-6));
    }

    #[test]
    fn education_goal_inflates_the_cost() {
        // ₹25 L at 8% for 10 years: 25 L × 1.08^10 = ₹53.97 L. At 12%, the
        // 120-month factor is ((1.01^120 − 1)/0.01) × 1.01 = 232.339, so the
        // SIP is 53,97,312 / 232.339 = ₹23,230.
        let e = plan_education(25e5, 10, 8.0, 12.0, 0.0, 0.0);
        assert!(near(e.future_cost, 5_397_312.5, 1.0), "{}", e.future_cost);
        assert!(
            near(e.sip.monthly_sip, 23_230.0, 2.0),
            "{}",
            e.sip.monthly_sip
        );
    }

    #[test]
    fn indian_grouping_puts_commas_in_lakhs_and_crores() {
        assert_eq!(indian_grouping(0.0), "0");
        assert_eq!(indian_grouping(999.0), "999");
        assert_eq!(indian_grouping(19_818.6), "19,819");
        assert_eq!(indian_grouping(250_000.0), "2,50,000");
        assert_eq!(indian_grouping(12_345_678.4), "1,23,45,678");
        assert_eq!(indian_grouping(-1_500.0), "-1,500");
        assert_eq!(western_grouping(999.0), "999");
        assert_eq!(western_grouping(1_234_567.4), "1,234,567");
        assert_eq!(western_grouping(-1_000.0), "-1,000");
    }
}
