//! GTT status changes between two looks at a broker's GTT list.
//!
//! `trade gtt list` and `trade sync --apply` save what they saw in
//! `gtt_seen_<broker>.json`; the next look reports GTTs that triggered, were
//! cancelled, expired or disappeared since. Pure apart from [`load`] and
//! [`save`].

use super::{BrokerKind, Gtt};
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// A broker's GTT status, normalised across brokers' spellings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GttState {
    Active,
    Triggered,
    Cancelled,
    Expired,
    Rejected,
    Other,
}

impl GttState {
    /// Map a raw status (`active`, `NEW`, `SENTTOEXCHANGE`, `CONFIRM`,
    /// `triggered`, `CANCELLED`, ...).
    pub fn of(status: Option<&str>) -> Self {
        let s = status
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase()
            .replace([' ', '-'], "_");
        match s.as_str() {
            "active" | "new" | "pending" | "confirm" | "open" | "trigger_pending" => {
                GttState::Active
            }
            "triggered" | "senttoexchange" | "sent_to_exchange" | "traded" | "executed"
            | "complete" | "completed" | "filled" => GttState::Triggered,
            "cancelled" | "canceled" | "deleted" | "disabled" => GttState::Cancelled,
            "expired" => GttState::Expired,
            "rejected" | "failed" => GttState::Rejected,
            _ => GttState::Other,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            GttState::Active => "active",
            GttState::Triggered => "triggered",
            GttState::Cancelled => "cancelled",
            GttState::Expired => "expired",
            GttState::Rejected => "rejected",
            GttState::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    /// Not in the previous list (created elsewhere, or since).
    New,
    /// Its status changed.
    Status,
    /// No longer listed: brokers drop triggered, cancelled or expired GTTs
    /// from the list after a while.
    Gone,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GttChange {
    pub kind: ChangeKind,
    pub id: String,
    pub symbol: String,
    pub side: Option<String>,
    pub qty: Option<f64>,
    pub trigger: Option<f64>,
    /// Raw status before and after (`None` when not listed).
    pub from: Option<String>,
    pub to: Option<String>,
    /// Normalised status now (for `gone`, the last one seen).
    pub state: GttState,
}

impl GttChange {
    fn of(g: &Gtt, kind: ChangeKind, from: Option<String>, to: Option<String>) -> Self {
        let state = GttState::of(to.as_deref().or(from.as_deref()));
        Self {
            kind,
            id: g.id.clone(),
            symbol: g.symbol.clone(),
            side: g.side.clone(),
            qty: g.qty,
            trigger: g.trigger,
            from,
            to,
            state,
        }
    }

    /// Worth a notification: a GTT that went away, or whose normalised
    /// state changed (`NEW` → `ACTIVE` is not news). Not a new GTT — the
    /// user just made it.
    pub fn notable(&self) -> bool {
        match self.kind {
            ChangeKind::New => false,
            ChangeKind::Gone => true,
            ChangeKind::Status => {
                GttState::of(self.from.as_deref()) != GttState::of(self.to.as_deref())
            }
        }
    }

    /// One line for the terminal or a notification.
    pub fn describe(&self) -> String {
        let what = format!(
            "GTT {} {} {}{}",
            self.id,
            self.symbol,
            self.side.as_deref().unwrap_or(""),
            self.qty.map_or(String::new(), |q| format!(" {q}"))
        );
        let what = what.trim_end();
        let fmt = |s: &Option<String>| s.clone().unwrap_or_else(|| "?".into());
        match self.kind {
            ChangeKind::New => format!("{what}: new ({})", fmt(&self.to)),
            ChangeKind::Status => format!(
                "{what}: {} → {} ({})",
                fmt(&self.from),
                fmt(&self.to),
                self.state.label()
            ),
            ChangeKind::Gone => format!(
                "{what}: no longer listed (was {}) — triggered, cancelled or expired",
                fmt(&self.from)
            ),
        }
    }
}

/// Changes from `old` to `new`, by GTT id. New GTTs first, in list order,
/// then status changes, then ones that went away.
pub fn diff(old: &[Gtt], new: &[Gtt]) -> Vec<GttChange> {
    let mut out = Vec::new();
    for n in new {
        match old.iter().find(|o| o.id == n.id) {
            None => out.push(GttChange::of(n, ChangeKind::New, None, n.status.clone())),
            Some(o) if o.status != n.status => out.push(GttChange::of(
                n,
                ChangeKind::Status,
                o.status.clone(),
                n.status.clone(),
            )),
            Some(_) => {}
        }
    }
    for o in old.iter().filter(|o| !new.iter().any(|n| n.id == o.id)) {
        out.push(GttChange::of(o, ChangeKind::Gone, o.status.clone(), None));
    }
    out.sort_by_key(|c| match c.kind {
        ChangeKind::New => 0,
        ChangeKind::Status => 1,
        ChangeKind::Gone => 2,
    });
    out
}

/// The GTT list as last seen.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Seen {
    /// When it was saved (RFC 3339, IST); `None` = never looked.
    #[serde(default)]
    pub checked_at: Option<String>,
    #[serde(default)]
    pub gtts: Vec<Gtt>,
}

fn file(kind: BrokerKind) -> String {
    format!("gtt_seen_{}.json", kind.id())
}

pub fn load(kind: BrokerKind) -> Result<Seen> {
    crate::store::load(&file(kind))
}

pub fn save(kind: BrokerKind, gtts: &[Gtt]) -> Result<()> {
    let seen = Seen {
        checked_at: Some(super::ist_now().to_rfc3339()),
        gtts: gtts.to_vec(),
    };
    crate::store::save(&file(kind), &seen, true)
}

/// Changes since `seen`; none on the first look (there is nothing to
/// compare with).
pub fn changes(seen: &Seen, now: &[Gtt]) -> Vec<GttChange> {
    if seen.checked_at.is_none() {
        return Vec::new();
    }
    diff(&seen.gtts, now)
}

/// The notification for notable changes, or `None` when there are none.
pub fn message(kind: BrokerKind, changes: &[GttChange]) -> Option<crate::notify::Message> {
    let lines: Vec<String> = changes
        .iter()
        .filter(|c| c.notable())
        .map(GttChange::describe)
        .collect();
    if lines.is_empty() {
        return None;
    }
    Some(crate::notify::Message::new(
        format!("StockWise: {} GTT update", kind.label()),
        lines.join("\n"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(id: &str, status: &str) -> Gtt {
        Gtt {
            id: id.into(),
            symbol: "INFY".into(),
            side: Some("SELL".into()),
            qty: Some(5.0),
            trigger: Some(1400.0),
            limit: Some(1395.0),
            status: Some(status.into()),
        }
    }

    #[test]
    fn statuses_normalise_across_brokers() {
        assert_eq!(GttState::of(Some("active")), GttState::Active);
        assert_eq!(GttState::of(Some("NEW")), GttState::Active);
        assert_eq!(GttState::of(Some("CONFIRM")), GttState::Active);
        assert_eq!(GttState::of(Some("triggered")), GttState::Triggered);
        assert_eq!(GttState::of(Some("SENTTOEXCHANGE")), GttState::Triggered);
        assert_eq!(GttState::of(Some("CANCELLED")), GttState::Cancelled);
        assert_eq!(GttState::of(Some("disabled")), GttState::Cancelled);
        assert_eq!(GttState::of(Some("expired")), GttState::Expired);
        assert_eq!(GttState::of(Some("weird")), GttState::Other);
        assert_eq!(GttState::of(None), GttState::Other);
    }

    #[test]
    fn diff_reports_new_changed_and_gone() {
        let old = vec![g("1", "active"), g("2", "active"), g("3", "active")];
        let new = vec![g("1", "active"), g("2", "triggered"), g("4", "active")];
        let d = diff(&old, &new);
        let kinds: Vec<(ChangeKind, &str)> = d.iter().map(|c| (c.kind, c.id.as_str())).collect();
        assert_eq!(
            kinds,
            vec![
                (ChangeKind::New, "4"),
                (ChangeKind::Status, "2"),
                (ChangeKind::Gone, "3")
            ]
        );
        assert_eq!(d[1].state, GttState::Triggered);
        assert!(
            d[1].describe().contains("active → triggered"),
            "{}",
            d[1].describe()
        );
        assert!(!d[0].notable() && d[1].notable() && d[2].notable());
        let msg = message(BrokerKind::Zerodha, &d).unwrap();
        assert!(msg.body.contains("GTT 2") && msg.body.contains("GTT 3"));
        assert!(!msg.body.contains("GTT 4"));
    }

    #[test]
    fn a_status_spelling_change_is_shown_but_not_notified() {
        // Angel One moves a fresh rule from NEW to ACTIVE.
        let d = diff(&[g("1", "NEW")], &[g("1", "ACTIVE")]);
        assert_eq!(d.len(), 1);
        assert!(!d[0].notable());
        assert!(message(BrokerKind::Angel, &d).is_none());
    }

    #[test]
    fn first_look_reports_nothing() {
        let now = vec![g("1", "active")];
        assert!(changes(&Seen::default(), &now).is_empty());
        let seen = Seen {
            checked_at: Some("2026-10-20T10:00:00+05:30".into()),
            gtts: vec![],
        };
        assert_eq!(changes(&seen, &now).len(), 1);
        assert!(message(BrokerKind::Angel, &changes(&seen, &now)).is_none());
    }
}
