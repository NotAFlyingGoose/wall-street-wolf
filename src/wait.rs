use std::{ops::Add, time::Duration};

use apca::api::v2::clock::{self, Clock};
use chrono::{DateTime, Local, Utc};
use tokio::time::{Interval, MissedTickBehavior};

use crate::backend::Backend;

pub(crate) enum MarketStatus {
    Open,
    AboutToClose,
}

pub(crate) struct Ticker {
    interval: Interval,
    clock: Clock,
    open_and_ready: bool,
}

impl Ticker {
    pub(crate) async fn new(
        backend: &dyn Backend,
        period: Duration,
    ) -> Result<Self, apca::RequestError<clock::GetError>> {
        let mut interval = tokio::time::interval(period);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        let clock = backend.clock_now().await;

        Ok(Self {
            interval,
            clock,
            open_and_ready: clock.open,
        })
    }

    pub(crate) async fn wait_for_open_or_tick(&mut self, backend: &dyn Backend) -> MarketStatus {
        let now = Utc::now();

        // `self.clock` was created yesterday, probably while the market was closed.
        // Because of that, it's `open` field isn't going to be accurate.
        // `self.open_and_ready` should be up-to-date. We maintain it ourselves to avoid constant
        // requests for the clock.
        if self.open_and_ready {
            let time_left = self
                .clock
                .next_close
                .signed_duration_since(now)
                .to_std()
                .unwrap();

            // gives us plenty of time to tick and still be able to execute some final logic
            let about_to_close = time_left <= self.interval.period() * 2;

            self.interval.tick().await;

            if about_to_close {
                self.open_and_ready = false;
                return MarketStatus::AboutToClose;
            }

            return MarketStatus::Open;
        }

        // if the market is still technically open, wait for it to close.
        // We have to check that we're within the bounds of the current market times.
        // However, the starting bound might be off by a day depending on when the program was
        // started. We might've started with an open market, in which case the `next_close` will be
        // today's close, but `next_open` will be for tomorrow.
        // If we started with a closed market, both `next_open` and `next_close` will be for today.
        if (self.clock.open || self.clock.next_open < now) && now < self.clock.next_close {
            let time_left = self
                .clock
                .next_close
                .signed_duration_since(now)
                .add(chrono::Duration::seconds(1))
                .to_std()
                .unwrap();

            tokio::time::sleep(time_left).await;
        }

        // now we can get the clock information for tomorrow
        self.clock = backend.clock_now().await;

        // we should only be here if the day ended
        assert!(!self.clock.open);

        let next_open: DateTime<Local> = DateTime::from(self.clock.next_open);
        let next_close: DateTime<Local> = DateTime::from(self.clock.next_close);

        tracing::info!(
            "Sleeping until the market opens on {} - {}",
            next_open.format("%A %d/%m/%Y at %I:%M %P"),
            next_close.format("%I:%M %P")
        );

        tokio::time::sleep(
            self.clock
                .next_open
                .signed_duration_since(Utc::now())
                .to_std()
                .unwrap(),
        )
        .await;

        tracing::info!("Sleep over");

        self.open_and_ready = true;

        MarketStatus::Open
    }
}
