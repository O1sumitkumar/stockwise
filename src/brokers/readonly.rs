//! A broker session that can only read.
//!
//! `trade selftest` and `trade sync` run against a real account, so they
//! get a [`ReadOnly`] wrapper instead of the session itself: every
//! order-changing method — place an order, create or cancel a GTT — fails
//! here without reaching the inner session, whatever the caller does. Read
//! methods pass through.

use super::{
    Broker, BrokerKind, Endpoint, Gtt, GttRequest, Holding, Instrument, Order, OrderAck,
    OrderRequest, Position, Profile, Shape, Trade,
};
use anyhow::{Result, bail};
use async_trait::async_trait;

pub struct ReadOnly {
    inner: Box<dyn Broker>,
}

impl ReadOnly {
    pub fn new(inner: Box<dyn Broker>) -> Self {
        Self { inner }
    }
}

/// The error every order-changing method returns.
fn refused(kind: BrokerKind, what: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "Refused: this {} session is read-only, so it cannot {what}. Nothing was sent.",
        kind.label()
    )
}

#[async_trait]
impl Broker for ReadOnly {
    fn kind(&self) -> BrokerKind {
        self.inner.kind()
    }

    async fn holdings(&self) -> Result<Vec<Holding>> {
        self.inner.holdings().await
    }

    async fn positions(&self) -> Result<Vec<Position>> {
        self.inner.positions().await
    }

    async fn orders(&self) -> Result<Vec<Order>> {
        self.inner.orders().await
    }

    async fn profile(&self) -> Result<Profile> {
        self.inner.profile().await
    }

    async fn trades(&self, since: Option<chrono::NaiveDate>) -> Result<Vec<Trade>> {
        self.inner.trades(since).await
    }

    async fn rows(&self, endpoint: Endpoint) -> Result<Vec<serde_json::Value>> {
        self.inner.rows(endpoint).await
    }

    fn shape(&self, endpoint: Endpoint) -> Option<Shape> {
        self.inner.shape(endpoint)
    }

    async fn resolve_instrument(&self, symbol: &str, exchange: &str) -> Result<Instrument> {
        self.inner.resolve_instrument(symbol, exchange).await
    }

    async fn place_order(&self, _order: &OrderRequest) -> Result<OrderAck> {
        Err(refused(self.kind(), "place an order"))
    }

    fn supports_gtt(&self) -> bool {
        self.inner.supports_gtt()
    }

    async fn gtt_create(&self, _gtt: &GttRequest) -> Result<String> {
        Err(refused(self.kind(), "create a GTT"))
    }

    async fn gtt_list(&self) -> Result<Vec<Gtt>> {
        if !self.inner.supports_gtt() {
            bail!("{} GTT orders are not supported yet", self.kind().label());
        }
        self.inner.gtt_list().await
    }

    async fn gtt_cancel(&self, _id: &str) -> Result<()> {
        Err(refused(self.kind(), "cancel a GTT"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brokers::{OrderKind, Side};
    use std::sync::{Arc, Mutex};

    /// Records every call that would change something at the broker.
    #[derive(Default, Clone)]
    struct Recorder {
        writes: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Broker for Recorder {
        fn kind(&self) -> BrokerKind {
            BrokerKind::Zerodha
        }
        async fn holdings(&self) -> Result<Vec<Holding>> {
            Ok(vec![Holding {
                symbol: "INFY".into(),
                qty: 1.0,
                ..Default::default()
            }])
        }
        async fn positions(&self) -> Result<Vec<Position>> {
            Ok(vec![])
        }
        async fn orders(&self) -> Result<Vec<Order>> {
            Ok(vec![])
        }
        async fn resolve_instrument(&self, symbol: &str, exchange: &str) -> Result<Instrument> {
            Ok(Instrument {
                symbol: symbol.into(),
                tradingsymbol: symbol.into(),
                token: "1".into(),
                exchange: exchange.into(),
            })
        }
        async fn place_order(&self, _order: &OrderRequest) -> Result<OrderAck> {
            self.writes.lock().unwrap().push("place_order".into());
            Ok(OrderAck {
                order_id: Some("X".into()),
            })
        }
        fn supports_gtt(&self) -> bool {
            true
        }
        async fn gtt_create(&self, _gtt: &GttRequest) -> Result<String> {
            self.writes.lock().unwrap().push("gtt_create".into());
            Ok("G".into())
        }
        async fn gtt_list(&self) -> Result<Vec<Gtt>> {
            Ok(vec![])
        }
        async fn gtt_cancel(&self, _id: &str) -> Result<()> {
            self.writes.lock().unwrap().push("gtt_cancel".into());
            Ok(())
        }
    }

    fn instrument() -> Instrument {
        Instrument {
            symbol: "INFY".into(),
            tradingsymbol: "INFY".into(),
            token: "408065".into(),
            exchange: "NSE".into(),
        }
    }

    #[tokio::test]
    async fn order_methods_fail_and_never_reach_the_session() {
        let rec = Recorder::default();
        let ro = ReadOnly::new(Box::new(rec.clone()));
        let order = OrderRequest {
            instrument: instrument(),
            side: Side::Buy,
            qty: 1,
            kind: OrderKind::Market,
        };
        let err = ro.place_order(&order).await.unwrap_err().to_string();
        assert!(err.contains("read-only"), "{err}");
        let gtt = GttRequest {
            instrument: instrument(),
            side: Side::Sell,
            qty: 1,
            trigger: 1400.0,
            limit: 1395.0,
            last_price: 1500.0,
        };
        assert!(ro.gtt_create(&gtt).await.is_err());
        assert!(ro.gtt_cancel("1").await.is_err());
        assert!(rec.writes.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn reads_pass_through() {
        let ro = ReadOnly::new(Box::new(Recorder::default()));
        assert_eq!(ro.kind(), BrokerKind::Zerodha);
        assert_eq!(ro.holdings().await.unwrap()[0].symbol, "INFY");
        assert!(ro.supports_gtt());
        assert!(ro.gtt_list().await.unwrap().is_empty());
        let i = ro.resolve_instrument("INFY", "NSE").await.unwrap();
        assert_eq!(
            (i.tradingsymbol.as_str(), i.exchange.as_str()),
            ("INFY", "NSE")
        );
    }
}
