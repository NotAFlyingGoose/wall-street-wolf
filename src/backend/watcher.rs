use std::{sync::Arc, time::Instant};

use apca::api::v2::updates::OrderUpdates;
use futures::StreamExt;
use tokio::task::JoinHandle;

use super::LiveInner;

pub(super) struct LiveOrderWatcher {
    handle: JoinHandle<()>,
}

impl LiveOrderWatcher {
    pub(crate) async fn new(inner: Arc<LiveInner>) -> Self {
        Self {
            handle: tokio::task::spawn(async move {
                let (mut stream, _) = inner.client.subscribe::<OrderUpdates>().await.unwrap();

                while let Some(res) = stream.next().await {
                    match res {
                        Ok(res) => match res {
                            Ok(res) => {
                                inner
                                    .account
                                    .positions
                                    .entry(res.order.symbol.into())
                                    .and_modify(|pos| {
                                        pos.order_in_progress = res.order.status.is_terminal();

                                        if res.order.status.is_terminal() {
                                            pos.owned += res.order.filled_quantity.clone();
                                            pos.buy_in_price = res
                                                .order
                                                .average_fill_price
                                                .clone()
                                                .unwrap_or_default();
                                            pos.timestamp = Instant::now()
                                        }
                                    })
                                    .or_insert_with(|| crate::Position {
                                        owned: res.order.filled_quantity,
                                        buy_in_price: res
                                            .order
                                            .average_fill_price
                                            .unwrap_or_default(),
                                        timestamp: Instant::now(),
                                        order_in_progress: res.order.status.is_terminal(),
                                    });
                            }
                            Err(why) => tracing::error!("order updates error: {why}"),
                        },
                        Err(why) => tracing::error!("order updates error: {why}"),
                    }
                }

                tracing::error!("closed");
            }),
        }
    }

    pub(crate) async fn open_if_closed(&mut self, inner: Arc<LiveInner>) {
        if self.handle.is_finished() {
            *self = Self::new(inner).await;
        }
    }

    pub(crate) async fn join(self) -> Result<(), tokio::task::JoinError> {
        self.handle.await
    }
}
