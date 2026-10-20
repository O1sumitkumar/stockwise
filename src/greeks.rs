//! Black-Scholes(-Merton) option pricing, greeks, and an implied-volatility
//! solver. Pure functions: European exercise, continuous rate and dividend
//! yield. Indian index/stock options are European; US equity options are
//! American, so for those the values are the usual close approximation.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OptionKind {
    Call,
    Put,
}

/// Inputs for one option. `t` is time to expiry in years; `rate`, `div_yield`
/// and `vol` are annual decimals (0.065 = 6.5%).
#[derive(Clone, Copy, Debug)]
pub struct BsInput {
    pub spot: f64,
    pub strike: f64,
    pub t: f64,
    pub rate: f64,
    pub div_yield: f64,
    pub vol: f64,
}

/// Greeks in the units traders quote: theta per calendar day, vega per one
/// volatility point (1%), rho per one rate point (1%).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Greeks {
    pub delta: f64,
    pub gamma: f64,
    pub theta: f64,
    pub vega: f64,
    pub rho: f64,
}

/// Standard normal probability density.
pub fn norm_pdf(x: f64) -> f64 {
    (-0.5 * x * x).exp() / (2.0 * std::f64::consts::PI).sqrt()
}

/// Standard normal cumulative distribution (via erfc, |error| < 1.2e-7).
pub fn norm_cdf(x: f64) -> f64 {
    0.5 * erfc(-x / std::f64::consts::SQRT_2)
}

/// Complementary error function, Chebyshev fit from Numerical Recipes
/// (`erfcc`), fractional error below 1.2e-7 everywhere.
fn erfc(x: f64) -> f64 {
    let z = x.abs();
    let t = 1.0 / (1.0 + 0.5 * z);
    let ans = t
        * (-z * z - 1.265_512_23
            + t * (1.000_023_68
                + t * (0.374_091_96
                    + t * (0.096_784_18
                        + t * (-0.186_288_06
                            + t * (0.278_868_07
                                + t * (-1.135_203_98
                                    + t * (1.488_515_87
                                        + t * (-0.822_152_23 + t * 0.170_872_77)))))))))
            .exp();
    if x >= 0.0 { ans } else { 2.0 - ans }
}

fn valid(inp: &BsInput) -> bool {
    inp.spot > 0.0 && inp.strike > 0.0 && inp.t > 0.0 && inp.vol > 0.0
}

fn d1_d2(inp: &BsInput) -> (f64, f64) {
    let sqrt_t = inp.t.sqrt();
    let d1 = ((inp.spot / inp.strike).ln()
        + (inp.rate - inp.div_yield + 0.5 * inp.vol * inp.vol) * inp.t)
        / (inp.vol * sqrt_t);
    (d1, d1 - inp.vol * sqrt_t)
}

/// Theoretical price. `None` for non-positive spot, strike, time or volatility.
pub fn price(kind: OptionKind, inp: &BsInput) -> Option<f64> {
    if !valid(inp) {
        return None;
    }
    let (d1, d2) = d1_d2(inp);
    let df_q = (-inp.div_yield * inp.t).exp();
    let df_r = (-inp.rate * inp.t).exp();
    Some(match kind {
        OptionKind::Call => inp.spot * df_q * norm_cdf(d1) - inp.strike * df_r * norm_cdf(d2),
        OptionKind::Put => inp.strike * df_r * norm_cdf(-d2) - inp.spot * df_q * norm_cdf(-d1),
    })
}

/// Delta, gamma, theta (per day), vega (per 1% vol) and rho (per 1% rate).
pub fn greeks(kind: OptionKind, inp: &BsInput) -> Option<Greeks> {
    if !valid(inp) {
        return None;
    }
    let (d1, d2) = d1_d2(inp);
    let sqrt_t = inp.t.sqrt();
    let df_q = (-inp.div_yield * inp.t).exp();
    let df_r = (-inp.rate * inp.t).exp();
    let pdf = norm_pdf(d1);

    let gamma = df_q * pdf / (inp.spot * inp.vol * sqrt_t);
    let vega = inp.spot * df_q * pdf * sqrt_t;
    let decay = -inp.spot * df_q * pdf * inp.vol / (2.0 * sqrt_t);
    let (delta, theta_year, rho) = match kind {
        OptionKind::Call => (
            df_q * norm_cdf(d1),
            decay - inp.rate * inp.strike * df_r * norm_cdf(d2)
                + inp.div_yield * inp.spot * df_q * norm_cdf(d1),
            inp.strike * inp.t * df_r * norm_cdf(d2),
        ),
        OptionKind::Put => (
            df_q * (norm_cdf(d1) - 1.0),
            decay + inp.rate * inp.strike * df_r * norm_cdf(-d2)
                - inp.div_yield * inp.spot * df_q * norm_cdf(-d1),
            -inp.strike * inp.t * df_r * norm_cdf(-d2),
        ),
    };
    Some(Greeks {
        delta,
        gamma,
        theta: theta_year / 365.0,
        vega: vega / 100.0,
        rho: rho / 100.0,
    })
}

/// Volatility that makes the model price equal `market_price` (the `vol` in
/// `inp` is ignored). Newton steps, falling back to bisection, within
/// 0.01%–500%. `None` if the price is outside the no-arbitrage bounds or no
/// volatility in range fits.
pub fn implied_vol(kind: OptionKind, market_price: f64, inp: &BsInput) -> Option<f64> {
    if !(market_price.is_finite() && market_price > 0.0)
        || inp.spot <= 0.0
        || inp.strike <= 0.0
        || inp.t <= 0.0
    {
        return None;
    }
    let df_q = (-inp.div_yield * inp.t).exp();
    let df_r = (-inp.rate * inp.t).exp();
    let (lower, upper) = match kind {
        OptionKind::Call => (
            (inp.spot * df_q - inp.strike * df_r).max(0.0),
            inp.spot * df_q,
        ),
        OptionKind::Put => (
            (inp.strike * df_r - inp.spot * df_q).max(0.0),
            inp.strike * df_r,
        ),
    };
    if market_price <= lower || market_price >= upper {
        return None;
    }

    let model = |vol: f64| price(kind, &BsInput { vol, ..*inp });
    let (mut lo, mut hi) = (1e-4, 5.0);
    let diff_lo = model(lo)? - market_price;
    let diff_hi = model(hi)? - market_price;
    if diff_lo > 0.0 || diff_hi < 0.0 {
        return None;
    }

    // Price rises with volatility, so the root stays bracketed by [lo, hi].
    let mut vol = 0.3;
    for _ in 0..100 {
        let diff = model(vol)? - market_price;
        if diff.abs() < 1e-8 {
            return Some(vol);
        }
        if diff > 0.0 {
            hi = vol;
        } else {
            lo = vol;
        }
        let vega = inp.spot * df_q * norm_pdf(d1_d2(&BsInput { vol, ..*inp }).0) * inp.t.sqrt();
        let newton = vol - diff / vega;
        vol = if vega > 1e-10 && newton > lo && newton < hi {
            newton
        } else {
            0.5 * (lo + hi)
        };
        if hi - lo < 1e-10 {
            break;
        }
    }
    Some(vol)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    #[test]
    fn norm_cdf_matches_tables() {
        assert!(close(norm_cdf(0.0), 0.5, 1e-7));
        assert!(close(norm_cdf(1.0), 0.841_344_75, 1e-6));
        assert!(close(norm_cdf(-1.96), 0.024_997_9, 1e-6));
        assert!(close(norm_cdf(3.0), 0.998_650_1, 1e-6));
    }

    #[test]
    fn prices_match_hull_example() {
        // Hull, Options, Futures & Other Derivatives: S=42, K=40, r=10%,
        // σ=20%, T=0.5 → call 4.76, put 0.81.
        let inp = BsInput {
            spot: 42.0,
            strike: 40.0,
            t: 0.5,
            rate: 0.10,
            div_yield: 0.0,
            vol: 0.20,
        };
        let call = price(OptionKind::Call, &inp).unwrap();
        let put = price(OptionKind::Put, &inp).unwrap();
        assert!(close(call, 4.759_422, 1e-4), "{call}");
        assert!(close(put, 0.808_599, 1e-4), "{put}");
        // Put-call parity: C - P = S - K e^{-rT}
        assert!(close(call - put, 42.0 - 40.0 * (-0.05f64).exp(), 1e-9));
    }

    #[test]
    fn greeks_match_hull_reference() {
        // Hull's running example: S=49, K=50, r=5%, σ=20%, T=20 weeks.
        // Delta 0.522, gamma 0.066, theta -4.31/yr, vega 12.1, rho 8.91.
        let inp = BsInput {
            spot: 49.0,
            strike: 50.0,
            t: 0.3846,
            rate: 0.05,
            div_yield: 0.0,
            vol: 0.20,
        };
        let g = greeks(OptionKind::Call, &inp).unwrap();
        assert!(close(g.delta, 0.521_602, 1e-5), "{g:?}");
        assert!(close(g.gamma, 0.065_545, 1e-5), "{g:?}");
        assert!(close(g.theta * 365.0, -4.305_390, 1e-4), "{g:?}");
        assert!(close(g.vega * 100.0, 12.105_243, 1e-4), "{g:?}");
        assert!(close(g.rho * 100.0, 8.906_574, 1e-4), "{g:?}");

        let p = greeks(OptionKind::Put, &inp).unwrap();
        assert!(close(p.delta, -0.478_398, 1e-5), "{p:?}");
        assert!(close(p.gamma, g.gamma, 1e-12));
        assert!(close(p.vega, g.vega, 1e-12));
        assert!(close(p.theta * 365.0, -1.853_006, 1e-4), "{p:?}");
        assert!(close(p.rho * 100.0, -9.957_166, 1e-4), "{p:?}");
    }

    #[test]
    fn dividend_yield_is_applied() {
        let inp = BsInput {
            spot: 100.0,
            strike: 95.0,
            t: 0.25,
            rate: 0.06,
            div_yield: 0.02,
            vol: 0.30,
        };
        assert!(close(
            price(OptionKind::Call, &inp).unwrap(),
            9.203_885,
            1e-4
        ));
        assert!(close(
            price(OptionKind::Put, &inp).unwrap(),
            3.288_271,
            1e-4
        ));
        let g = greeks(OptionKind::Call, &inp).unwrap();
        assert!(close(g.delta, 0.682_253, 1e-5), "{g:?}");
        assert!(close(g.theta * 365.0, -12.771_054, 1e-3), "{g:?}");
    }

    #[test]
    fn implied_vol_round_trips() {
        for &(kind, strike, vol) in &[
            (OptionKind::Call, 90.0, 0.15),
            (OptionKind::Call, 100.0, 0.25),
            (OptionKind::Call, 130.0, 0.60),
            (OptionKind::Put, 80.0, 0.45),
            (OptionKind::Put, 105.0, 0.12),
            (OptionKind::Put, 100.0, 1.50),
        ] {
            let inp = BsInput {
                spot: 100.0,
                strike,
                t: 30.0 / 365.0,
                rate: 0.065,
                div_yield: 0.0,
                vol,
            };
            let p = price(kind, &inp).unwrap();
            let iv = implied_vol(kind, p, &inp).unwrap();
            assert!(close(iv, vol, 1e-6), "{kind:?} K={strike}: {iv} vs {vol}");
        }
    }

    #[test]
    fn implied_vol_rejects_impossible_prices() {
        let inp = BsInput {
            spot: 100.0,
            strike: 90.0,
            t: 0.1,
            rate: 0.05,
            div_yield: 0.0,
            vol: 0.0,
        };
        // Below intrinsic value, above the spot, zero, and expired.
        assert_eq!(implied_vol(OptionKind::Call, 5.0, &inp), None);
        assert_eq!(implied_vol(OptionKind::Call, 150.0, &inp), None);
        assert_eq!(implied_vol(OptionKind::Call, 0.0, &inp), None);
        assert_eq!(
            implied_vol(OptionKind::Call, 12.0, &BsInput { t: 0.0, ..inp }),
            None
        );
    }

    #[test]
    fn invalid_inputs_give_none() {
        let inp = BsInput {
            spot: 100.0,
            strike: 100.0,
            t: 0.0,
            rate: 0.05,
            div_yield: 0.0,
            vol: 0.2,
        };
        assert!(price(OptionKind::Call, &inp).is_none());
        assert!(
            greeks(
                OptionKind::Put,
                &BsInput {
                    vol: 0.0,
                    t: 1.0,
                    ..inp
                }
            )
            .is_none()
        );
    }
}
