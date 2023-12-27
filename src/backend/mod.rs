mod endpoints;
mod live;
mod test;
mod watcher;

use std::collections::HashMap;

use apca::{
    api::v2::{
        clock::Clock,
        order::{Amount, Side},
    },
    data::v2::{bars, Feed},
};
use async_trait::async_trait;
use num_decimal::Num;

use crate::{AccountState, Symbol, TimePeriod};

pub(crate) use live::*;

pub(crate) struct Stats {
    pub(crate) current_equity: Num,
    pub(crate) last_equity: Num,
}

#[async_trait]
pub(crate) trait Backend {
    async fn submit_order(&self, symbol: Symbol, side: Side, amount: Amount);

    async fn cancel_all_open_orders(&self);

    async fn clock_now(&self) -> Clock;

    async fn all_active_assets(&self) -> Vec<Symbol>;

    async fn all_latest_prices(&self, symbols: Vec<Symbol>) -> HashMap<Symbol, Num>;

    async fn all_latest_bars(
        &self,
        symbols: Vec<Symbol>,
        period: TimePeriod,
        feed: Feed,
    ) -> HashMap<Symbol, Vec<bars::Bar>> {
        let bars = symbols.into_iter().map(|symbol| async {
            let bars = self.latest_bars(symbol.clone(), period, feed).await;
            (symbol, bars)
        });
        futures::future::join_all(bars).await.into_iter().collect()
    }

    async fn latest_bars(&self, symbol: Symbol, period: TimePeriod, feed: Feed) -> Vec<bars::Bar>;

    async fn final_stats(&self) -> Stats;

    async fn open(&self);

    async fn close(&self);

    async fn sell_all_positions<F>(&self, filter: F)
    where
        Self: Sized,
        F: Fn(&Symbol) -> bool + Send,
    {
        let account = self.account_data();

        if account.positions.is_empty() {
            return;
        }

        for (symbol, pos) in account.positions.clone() {
            if filter(&symbol) {
                self.submit_order(symbol, Side::Sell, Amount::quantity(pos.owned))
                    .await;
            }
        }
    }

    fn account_data(&self) -> &AccountState;
}
