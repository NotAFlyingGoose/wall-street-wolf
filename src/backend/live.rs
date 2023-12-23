use std::{collections::HashMap, sync::Arc, time::Instant};

use apca::{
    api::v2::{
        account,
        asset::{self, Exchange},
        assets,
        clock::{self, Clock},
        order::{self, Amount, Side, TimeInForce},
        positions,
    },
    data::v2::{bars, Feed},
};
use async_trait::async_trait;
use chrono::Utc;
use num_decimal::Num;
use tokio::sync::Mutex;

use crate::{AccountState, Position, Symbol, TimePeriod};

use super::{endpoints, watcher::LiveOrderWatcher, Backend, Stats};

pub(super) struct LiveInner {
    pub(super) client: apca::Client,
    pub(super) account: AccountState,
}

pub(crate) struct LiveBackend {
    inner: Arc<LiveInner>,
    watcher: Mutex<LiveOrderWatcher>,
}

impl LiveBackend {
    pub(crate) async fn new() -> Self {
        let api_info = apca::ApiInfo::from_env().unwrap();
        let client = apca::Client::new(api_info);

        let now = Instant::now();

        let account = AccountState {
            positions: client
                .issue::<positions::Get>(&())
                .await
                .unwrap()
                .into_iter()
                .map(|position| {
                    (
                        position.symbol.into(),
                        Position {
                            owned: position.quantity,
                            buy_in_price: position.current_price.unwrap_or_default(),
                            timestamp: now,
                            order_in_progress: false,
                        },
                    )
                })
                .collect(),
        };

        tracing::debug!("account: {}", account);

        let inner = Arc::new(LiveInner { client, account });

        Self {
            watcher: LiveOrderWatcher::new(inner.clone()).await.into(),
            inner,
        }
    }
}

#[async_trait]
impl Backend for LiveBackend {
    async fn submit_order(&self, symbol: Symbol, side: Side, amount: Amount) {
        let amount_str = match &amount {
            Amount::Quantity { quantity } => format!("{}", quantity),
            Amount::Notional { notional } => format!("${}", notional),
        };

        let request = order::OrderReqInit {
            time_in_force: match symbol {
                Symbol::Crypto { .. } => TimeInForce::UntilCanceled,
                Symbol::Stock { .. } => TimeInForce::Day,
            },
            ..Default::default()
        }
        .init(symbol.clone().ticker(), side, amount);

        self.inner
            .client
            .issue::<order::Post>(&request)
            .await
            .unwrap();

        match side {
            Side::Buy => tracing::info!("Bought {amount_str} of {symbol}"),
            Side::Sell => tracing::info!("Sold {amount_str} of {symbol}"),
        }
    }

    async fn cancel_all_open_orders(&self) {
        let cancelled_orders = self
            .inner
            .client
            .issue::<endpoints::CancelAllOrders>(&())
            .await
            .unwrap();

        if !cancelled_orders.0.is_empty() {
            tracing::debug!("Cancelled {} orders", cancelled_orders.0.len());
        }
    }

    async fn clock_now(&self) -> Clock {
        self.inner.client.issue::<clock::Get>(&()).await.unwrap()
    }

    async fn all_active_assets(&self) -> Vec<Symbol> {
        self.inner
            .client
            .issue::<assets::Get>(
                &assets::AssetsReqInit {
                    status: asset::Status::Active,
                    ..Default::default()
                }
                .init(),
            )
            .await
            .unwrap()
            .into_iter()
            .filter(|asset| asset.tradable && asset.exchange != Exchange::Otc)
            .map(|asset| asset.symbol.into())
            .collect()
    }

    async fn all_latest_prices(&self, symbols: Vec<Symbol>) -> HashMap<Symbol, Num> {
        let request = endpoints::LastTradesReqInit {
            // feed: Some(Feed::IEX),
            ..Default::default()
        }
        .init(
            symbols
                .into_iter()
                .map(|symbol| symbol.ticker().to_string()),
        );

        let data = self
            .inner
            .client
            .issue::<endpoints::GetLastTrades>(&request)
            .await
            .unwrap();

        data.into_iter()
            .map(|(symbol, quote)| (symbol.into(), quote.price))
            .collect()
    }

    async fn latest_bars(&self, symbol: Symbol, period: TimePeriod, feed: Feed) -> Vec<bars::Bar> {
        let to = Utc::now()
            .checked_sub_signed(chrono::Duration::minutes(match feed {
                Feed::IEX => 1,
                Feed::SIP => 5,
                _ => 0,
            }))
            .unwrap();
        let from = to.checked_sub_signed(period.to_chrono()).unwrap();

        let request = bars::BarsReqInit {
            feed: Some(feed),
            ..Default::default()
        }
        .init(symbol.ticker(), from, to, period.timeframe);

        let data = self
            .inner
            .client
            .issue::<bars::Get>(&request)
            .await
            .unwrap();
        if data.next_page_token.is_some() {
            tracing::error!("more pages than expected");
        }

        // calculate the average of all the trades
        data.bars
    }

    async fn final_stats(&self) -> Stats {
        let account = self.inner.client.issue::<account::Get>(&()).await.unwrap();

        Stats {
            current_equity: account.equity,
            last_equity: account.last_equity,
        }
    }

    async fn open_if_closed(&self) {
        self.watcher
            .lock()
            .await
            .open_if_closed(self.inner.clone())
            .await
    }

    fn account_data(&self) -> &AccountState {
        &self.inner.account
    }
}
