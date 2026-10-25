//! Practice mode for the paper-trading simulator: historical replays,
//! goal-based challenges, risk-adjusted scores, badges and streaks, and a
//! shareable result card. Paper money only — nothing here can reach a
//! broker. See docs/simulator.md.
//!
//! - [`scenario`]: built-in and custom replay scenarios (TOML).
//! - [`tape`]: the replay's cached daily bars; reads stop at the replay date.
//! - [`replay`]: orders, next-open fills with the backtest cost models,
//!   stops and targets, daily marks.
//! - [`challenge`]: data-driven rules and goals; [`live`] runs them over
//!   live paper sessions.
//! - [`score`]: the 0–100 score, grades and badges.
//! - [`profile`]: badges, streaks and best scores on disk.
//! - [`card`]: the text and SVG result cards.

pub mod card;
pub mod challenge;
pub mod live;
pub mod profile;
pub mod replay;
pub mod scenario;
pub mod score;
pub mod tape;
