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

use super::{Backend, Stats};

pub(crate) struct TestBackend {
    client: apca::Client,
    account: AccountState,
}

impl TestBackend {
    async fn new() -> Self {
        let api_info = apca::ApiInfo::from_env().unwrap();

        Self {
            client: apca::Client::new(api_info),
            account: AccountState {
                positions: Default::default(),
            },
        }
    }
}

#[async_trait]
impl Backend for TestBackend {
    async fn submit_order(&self, symbol: Symbol, side: Side, amount: Amount) {
        todo!()
    }

    async fn cancel_all_open_orders(&self) {
        todo!()
    }

    async fn clock_now(&self) -> Clock {
        todo!()
    }

    async fn all_active_assets(&self) -> Vec<Symbol> {
        todo!()
    }

    async fn all_latest_prices(&self, symbols: Vec<Symbol>) -> HashMap<Symbol, Num> {
        todo!()
    }

    async fn latest_bars(&self, symbol: Symbol, period: TimePeriod, feed: Feed) -> Vec<bars::Bar> {
        todo!()
    }

    async fn final_stats(&self) -> Stats {
        todo!()
    }

    async fn open(&self) {}

    async fn close(&self) {}

    fn account_data(&self) -> &AccountState {
        &self.account
    }
}
