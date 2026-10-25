//! The shareable result card: plain text for a chat message, SVG for an
//! image. Both are built from a stored [`RunResult`] — nothing is fetched,
//! nothing is uploaded — and both carry the paper-trading disclaimer.

use super::profile::RunResult;
use super::score;
use crate::display::{display_width, pad_end, pad_start};
use crate::i18n::{Msg, t, tf};

/// Inner width of the text card, in terminal columns.
const WIDTH: usize = 46;

fn kind_label(run: &RunResult) -> &'static str {
    if run.kind == "live" {
        t(Msg::PracticeCardLive)
    } else {
        t(Msg::PracticeCardReplay)
    }
}

fn pct(v: f64) -> String {
    format!("{v:+.1}%")
}

fn ratio(v: Option<f64>) -> String {
    v.map_or("—".into(), |r| format!("{r:.2}"))
}

/// Cut `text` to `width` columns, ending in "…" when it was longer.
fn fit(text: &str, width: usize) -> String {
    if display_width(text) <= width {
        return text.to_string();
    }
    let mut out = String::new();
    for c in text.chars() {
        if display_width(&out) + display_width(&c.to_string()) + 1 > width {
            break;
        }
        out.push(c);
    }
    out.push('…');
    out
}

/// Two series on one scale as block characters.
fn sparklines(a: &[f64], b: &[f64]) -> (String, String) {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let all = a.iter().chain(b);
    let lo = all.clone().copied().fold(f64::INFINITY, f64::min);
    let hi = all.copied().fold(f64::NEG_INFINITY, f64::max);
    let line = |v: &[f64]| -> String {
        v.iter()
            .map(|x| {
                if hi > lo {
                    BARS[(((x - lo) / (hi - lo)) * 7.0).round().clamp(0.0, 7.0) as usize]
                } else {
                    BARS[3]
                }
            })
            .collect()
    };
    (line(a), line(b))
}

fn score_line(run: &RunResult) -> String {
    match (run.scorecard.score, &run.scorecard.grade) {
        (Some(s), Some(g)) => format!(
            "{} {s}/100 · {} {g}",
            t(Msg::PracticeScore),
            t(Msg::PracticeGrade)
        ),
        _ => tf(Msg::PracticeNotScored, &[&score::MIN_DAYS]),
    }
}

fn badge_titles(run: &RunResult) -> Vec<&'static str> {
    run.badges
        .iter()
        .filter_map(|id| score::badge(id))
        .map(|b| t(b.title))
        .collect()
}

/// The text card, one string per line, no colour codes.
pub fn text(run: &RunResult) -> Vec<String> {
    let c = &run.scorecard;
    let bench = fit(&run.benchmark_name, 12);
    let row = |label: &str, you: String, them: String| {
        format!(
            "{}{}{}",
            pad_end(&fit(label, 18), 18),
            pad_start(&you, 10),
            pad_start(&them, 16)
        )
    };
    let mut body = vec![
        fit(&format!("StockWise · {}", kind_label(run)), WIDTH),
        fit(&run.title, WIDTH),
        fit(
            &format!(
                "{} → {} · {}",
                run.from,
                run.to,
                tf(Msg::PracticeTradingDays, &[&c.days])
            ),
            WIDTH,
        ),
        String::new(),
        fit(&score_line(run), WIDTH),
        row("", t(Msg::PracticeYou).to_string(), bench.clone()),
        row(
            t(Msg::PracticeReturn),
            pct(c.return_percent),
            pct(c.benchmark_return_percent),
        ),
        row(
            t(Msg::PracticeMaxDrawdown),
            format!("{:.1}%", c.max_drawdown_percent),
            format!("{:.1}%", c.benchmark_max_drawdown_percent),
        ),
        row("Sharpe", ratio(c.sharpe), ratio(c.benchmark_sharpe)),
        row(
            t(Msg::PracticeInvested),
            format!("{:.0}%", c.exposure_percent),
            String::new(),
        ),
    ];
    if run.curve.len() > 1 && run.benchmark_curve.len() == run.curve.len() {
        let thin = |v: &[f64]| score::rebased(v, 30);
        let (you, them) = sparklines(&thin(&run.curve), &thin(&run.benchmark_curve));
        body.push(format!(
            "{} {you}",
            pad_end(&fit(t(Msg::PracticeYou), 12), 12)
        ));
        body.push(format!("{} {them}", pad_end(&bench, 12)));
    }
    body.push(String::new());
    let badges = badge_titles(run);
    if badges.is_empty() && run.challenges.is_empty() {
        body.push(t(Msg::PracticeNoBadges).to_string());
    }
    for b in badges {
        body.push(fit(&format!("★ {b}"), WIDTH));
    }
    for ch in &run.challenges {
        let mark = if ch.passed { '✓' } else { '✗' };
        let verdict = if ch.passed {
            t(Msg::PracticePassed)
        } else {
            t(Msg::PracticeFailed)
        };
        body.push(fit(
            &format!(
                "{mark} {}: {} — {verdict}",
                t(Msg::PracticeChallenge),
                ch.title
            ),
            WIDTH,
        ));
    }
    body.push(String::new());
    body.push(fit(t(score::DISCLAIMER), WIDTH));

    let rule = "─".repeat(WIDTH + 2);
    let mut out = vec![format!("╭{rule}╮")];
    for line in body {
        out.push(format!("│ {} │", pad_end(&line, WIDTH)));
    }
    out.push(format!("╰{rule}╯"));
    out
}

/// Escape text for SVG.
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn grade_colour(grade: Option<&str>) -> &'static str {
    match grade {
        Some("A+" | "A") => "#50fa7b",
        Some("B") => "#8be9fd",
        Some("C") => "#f1fa8c",
        Some("D") => "#ffb86c",
        Some(_) => "#ff5555",
        None => "#6272a4",
    }
}

/// SVG polyline points for `v` inside the box (x, y, w, h), on the scale
/// lo..hi.
fn points(v: &[f64], (x, y, w, h): (f64, f64, f64, f64), lo: f64, hi: f64) -> String {
    let n = v.len().max(2) - 1;
    v.iter()
        .enumerate()
        .map(|(i, p)| {
            let px = x + w * i as f64 / n as f64;
            let py = if hi > lo {
                y + h - (p - lo) / (hi - lo) * h
            } else {
                y + h / 2.0
            };
            format!("{px:.1},{py:.1}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The SVG card (640 × 360), self-contained: no fonts or images to load.
pub fn svg(run: &RunResult) -> String {
    const FG: &str = "#f8f8f2";
    const DIM: &str = "#8a8fa8";
    let c = &run.scorecard;
    let mut s = Vec::new();
    s.push(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="640" height="360" viewBox="0 0 640 360">"#
            .to_string(),
    );
    s.push(
        "<style>text{font-family:'JetBrains Mono','SFMono-Regular',Menlo,Consolas,'DejaVu Sans Mono',\
         'Noto Sans Devanagari',monospace;font-size:13px}.b{font-weight:700}</style>"
            .to_string(),
    );
    s.push(r##"<rect width="640" height="360" rx="16" fill="#282a36"/>"##.to_string());
    s.push(format!(
        r#"<text x="28" y="38" fill="{DIM}" style="font-size:11px;letter-spacing:1px">STOCKWISE · {}</text>"#,
        esc(&kind_label(run).to_uppercase())
    ));
    s.push(format!(
        r#"<text x="28" y="68" fill="{FG}" class="b" style="font-size:22px">{}</text>"#,
        esc(&fit(&run.title, 34))
    ));
    s.push(format!(
        r#"<text x="28" y="92" fill="{DIM}">{}</text>"#,
        esc(&fit(
            &format!(
                "{} → {} · {}",
                run.from,
                run.to,
                tf(Msg::PracticeTradingDays, &[&c.days])
            ),
            62
        ))
    ));

    // Score ring.
    let colour = grade_colour(c.grade.as_deref());
    let (cx, cy, r) = (560.0, 74.0, 46.0);
    let frac = c.score.map_or(0.0, |p| p as f64 / 100.0);
    let circ = 2.0 * std::f64::consts::PI * r;
    s.push(format!(
        r##"<circle cx="{cx}" cy="{cy}" r="{r}" fill="none" stroke="#44475a" stroke-width="8"/>"##
    ));
    s.push(format!(
        r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill="none" stroke="{colour}" stroke-width="8" stroke-linecap="round" stroke-dasharray="{:.1} {circ:.1}" transform="rotate(-90 {cx} {cy})"/>"#,
        circ * frac
    ));
    let (big, small) = match (c.score, &c.grade) {
        (Some(p), Some(g)) => (p.to_string(), format!("{} {g}", t(Msg::PracticeGrade))),
        _ => ("—".to_string(), t(Msg::PracticeScore).to_string()),
    };
    s.push(format!(
        r#"<text x="{cx}" y="{}" fill="{FG}" class="b" text-anchor="middle" style="font-size:30px">{}</text>"#,
        cy + 6.0,
        esc(&big)
    ));
    s.push(format!(
        r#"<text x="{cx}" y="{}" fill="{colour}" class="b" text-anchor="middle" style="font-size:12px">{}</text>"#,
        cy + 24.0,
        esc(&small)
    ));

    // Metrics: you vs the benchmark.
    let bench = fit(&run.benchmark_name, 14);
    let y0 = 140.0;
    s.push(format!(
        r#"<text x="196" y="{y0}" fill="{DIM}" text-anchor="end">{}</text><text x="300" y="{y0}" fill="{DIM}" text-anchor="end">{}</text>"#,
        esc(t(Msg::PracticeYou)),
        esc(&bench)
    ));
    let good = c.return_percent >= c.benchmark_return_percent;
    let rows = [
        (
            t(Msg::PracticeReturn).to_string(),
            pct(c.return_percent),
            pct(c.benchmark_return_percent),
            if good { "#50fa7b" } else { "#ff79c6" },
        ),
        (
            t(Msg::PracticeMaxDrawdown).to_string(),
            format!("{:.1}%", c.max_drawdown_percent),
            format!("{:.1}%", c.benchmark_max_drawdown_percent),
            FG,
        ),
        (
            "Sharpe".to_string(),
            ratio(c.sharpe),
            ratio(c.benchmark_sharpe),
            FG,
        ),
        (
            t(Msg::PracticeInvested).to_string(),
            format!("{:.0}%", c.exposure_percent),
            String::new(),
            FG,
        ),
    ];
    for (i, (label, you, them, colour)) in rows.iter().enumerate() {
        let y = y0 + 26.0 * (i + 1) as f64;
        s.push(format!(
            r#"<text x="28" y="{y}" fill="{DIM}">{}</text><text x="196" y="{y}" fill="{colour}" class="b" text-anchor="end">{}</text><text x="300" y="{y}" fill="{FG}" text-anchor="end">{}</text>"#,
            esc(&fit(label, 16)),
            esc(you),
            esc(them)
        ));
    }

    // Curves on one scale.
    if run.curve.len() > 1 && run.benchmark_curve.len() == run.curve.len() {
        let area = (340.0, 124.0, 272.0, 112.0);
        s.push(format!(
            r##"<rect x="{}" y="{}" width="{}" height="{}" rx="8" fill="#21222c"/>"##,
            area.0 - 8.0,
            area.1 - 8.0,
            area.2 + 16.0,
            area.3 + 16.0
        ));
        let all = run.curve.iter().chain(&run.benchmark_curve);
        let lo = all.clone().copied().fold(f64::INFINITY, f64::min);
        let hi = all.copied().fold(f64::NEG_INFINITY, f64::max);
        s.push(format!(
            r##"<polyline fill="none" stroke="#6272a4" stroke-width="2" points="{}"/>"##,
            points(&run.benchmark_curve, area, lo, hi)
        ));
        s.push(format!(
            r#"<polyline fill="none" stroke="{}" stroke-width="2.5" points="{}"/>"#,
            if good { "#50fa7b" } else { "#ff79c6" },
            points(&run.curve, area, lo, hi)
        ));
        s.push(format!(
            r##"<text x="{}" y="{}" text-anchor="end" style="font-size:11px"><tspan fill="{}">— {}</tspan><tspan fill="#6272a4">   — {}</tspan></text>"##,
            area.0 + area.2,
            area.1 + area.3 + 22.0,
            if good { "#50fa7b" } else { "#ff79c6" },
            esc(t(Msg::PracticeYou)),
            esc(&bench)
        ));
    }

    // Badges and challenges as chips.
    let mut chips: Vec<(String, &str)> = badge_titles(run)
        .into_iter()
        .map(|b| (format!("★ {b}"), "#f1fa8c"))
        .collect();
    for ch in &run.challenges {
        let mark = if ch.passed { '✓' } else { '✗' };
        let colour = if ch.passed { "#50fa7b" } else { "#ff5555" };
        chips.push((format!("{mark} {}", ch.title), colour));
    }
    if chips.is_empty() {
        chips.push((t(Msg::PracticeNoBadges).to_string(), DIM));
    }
    let (mut x, mut y) = (28.0, 284.0);
    let mut shown = 0;
    for (label, colour) in &chips {
        let w = display_width(label) as f64 * 7.4 + 20.0;
        if x + w > 612.0 {
            if y > 284.0 {
                break;
            }
            x = 28.0;
            y += 30.0;
            if x + w > 612.0 {
                break;
            }
        }
        s.push(format!(
            r##"<rect x="{x:.1}" y="{:.1}" width="{w:.1}" height="22" rx="11" fill="#44475a"/><text x="{:.1}" y="{:.1}" fill="{colour}" style="font-size:12px">{}</text>"##,
            y - 15.0,
            x + 10.0,
            y,
            esc(label)
        ));
        x += w + 8.0;
        shown += 1;
    }
    if shown < chips.len() {
        s.push(format!(
            r#"<text x="612" y="{y}" fill="{DIM}" text-anchor="end" style="font-size:12px">+{}</text>"#,
            chips.len() - shown
        ));
    }

    s.push(format!(
        r#"<text x="28" y="344" fill="{DIM}" style="font-size:11px">{}</text>"#,
        esc(t(score::DISCLAIMER))
    ));
    s.push("</svg>".to_string());
    s.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::practice::profile::ChallengeSummary;
    use crate::practice::profile::tests::run;

    fn sample() -> RunResult {
        let mut r = run(
            "covid-crash-2020",
            Some(78),
            true,
            &["kept-your-stops", "survived-the-crash"],
        );
        r.title = "The COVID crash <&>".into();
        r.scorecard.return_percent = 4.2;
        r.scorecard.benchmark_return_percent = -9.8;
        r.scorecard.max_drawdown_percent = 8.1;
        r.scorecard.benchmark_max_drawdown_percent = 38.4;
        r.scorecard.sharpe = Some(0.91);
        r.challenges.push(ChallengeSummary {
            name: "steady-hands".into(),
            title: "Steady hands".into(),
            passed: true,
            reason: "completed".into(),
        });
        r
    }

    #[test]
    fn text_card_is_a_closed_box_with_the_essentials() {
        let lines = text(&sample());
        let width = display_width(&lines[0]);
        for l in &lines {
            assert_eq!(display_width(l), width, "{l}");
        }
        let all = lines.join("\n");
        for needle in [
            "Score 78/100 · Grade B",
            "+4.2%",
            "-9.8%",
            "38.4%",
            "0.91",
            "★ Kept your stops",
            "★ Survived the crash",
            "✓ Challenge: Steady hands — passed",
            "Paper trading — not financial advice.",
        ] {
            assert!(all.contains(needle), "{needle}\n{all}");
        }
    }

    #[test]
    fn unscored_runs_say_so() {
        let lines = text(&run("x", None, false, &[])).join("\n");
        assert!(lines.contains("Not scored"), "{lines}");
        assert!(lines.contains("not financial advice"));
    }

    #[test]
    fn svg_card_is_escaped_and_carries_the_disclaimer() {
        let svg = svg(&sample());
        assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
        assert!(svg.contains("The COVID crash &lt;&amp;&gt;"));
        assert!(!svg.contains("<&>"));
        assert!(svg.contains("Paper trading — not financial advice."));
        assert!(svg.contains(">78<"));
        assert!(svg.contains("Grade B"));
        assert!(svg.contains("<polyline"));
        assert!(svg.contains("★ Kept your stops"));
        // Deterministic.
        assert_eq!(svg, super::svg(&sample()));
    }

    #[test]
    fn fit_truncates_by_width() {
        assert_eq!(fit("abc", 5), "abc");
        assert_eq!(fit("abcdefgh", 5), "abcd…");
    }
}
