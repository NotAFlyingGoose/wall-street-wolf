mod backend;
mod scrape;
mod stats;
mod wait;

use std::{
    fmt::{Debug, Display, Write},
    sync::Arc,
    time::{Duration, Instant},
};

use apca::{
    api::v2::order::{Amount, Side},
    data::v2::{bars::TimeFrame, Feed},
};
use dashmap::DashMap;
use itertools::Itertools;
use num_decimal::Num;
use tracing_subscriber::{prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt};

use crate::{
    backend::{Backend, LiveBackend},
    stats::Statistics,
    wait::{MarketStatus, Ticker},
};

const KNOWN_CRYPTOS: &[&str] = &[
    "BTC", "ETH", "PAXG", "BCH", "AAVE", "LTC", "LINK", "UNI", "SHIB", "USDT",
];

#[derive(Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
enum Symbol {
    Stock { ticker: String },
    Crypto { ticker: String },
}

impl Symbol {
    fn ticker(&self) -> &str {
        match self {
            Self::Stock { ticker } => ticker,
            Self::Crypto { ticker } => ticker,
        }
    }
}

impl Display for Symbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self.ticker(), f)
    }
}

impl Debug for Symbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stock { ticker } => f.write_fmt(format_args!("Stock {}", ticker)),
            Self::Crypto { ticker } => f.write_fmt(format_args!("Crypto {}", ticker)),
        }
    }
}

impl<S> From<S> for Symbol
where
    S: Into<String> + Ord,
{
    fn from(value: S) -> Self {
        let mut value: String = value.into();
        value.retain(|ch| ch.is_alphabetic());

        if KNOWN_CRYPTOS.iter().any(|known| value.contains(known)) {
            Self::Crypto { ticker: value }
        } else {
            Self::Stock { ticker: value }
        }
    }
}

// represents a repeating time frame but one that only lasts for so long
//
// e.g. if the period repeats every minute, but has a length of 5:
//
// 1st minute ...
// 2nd minute ...
// 3rd minute ...
// 4th minute ...
// 5th minute done!
#[derive(Debug, Clone, Copy)]
struct TimePeriod {
    timeframe: TimeFrame,
    len: u64,
}

impl TimePeriod {
    #[allow(unused)]
    fn minutes(len: u64) -> Self {
        Self {
            timeframe: TimeFrame::OneMinute,
            len,
        }
    }

    #[allow(unused)]
    fn hours(len: u64) -> Self {
        Self {
            timeframe: TimeFrame::OneHour,
            len,
        }
    }

    #[allow(unused)]
    fn days(len: u64) -> Self {
        Self {
            timeframe: TimeFrame::OneDay,
            len,
        }
    }

    fn to_chrono(self) -> chrono::Duration {
        match self.timeframe {
            TimeFrame::OneMinute => chrono::Duration::minutes(self.len as i64),
            TimeFrame::OneHour => chrono::Duration::hours(self.len as i64),
            TimeFrame::OneDay => chrono::Duration::days(self.len as i64),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, PartialOrd, Eq, Ord)]
struct Position {
    owned: Num,
    buy_in_price: Num,
    timestamp: Instant,
    order_in_progress: bool,
}

#[derive(Debug)]
struct AccountState {
    positions: DashMap<Symbol, Position>,
}

impl Display for AccountState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_char('{')?;
        for (idx, entry) in self.positions.iter().enumerate() {
            let (symbol, position) = entry.pair();
            f.write_str("\n  ")?;
            Display::fmt(&symbol, f)?;
            f.write_str(" (")?;
            Display::fmt(&position.owned.to_f64().unwrap(), f)?;
            write!(f, " @ ${:.2})", &position.buy_in_price.to_f64().unwrap())?;

            if idx < self.positions.len() - 1 {
                f.write_char(',')?;
            } else {
                f.write_char('\n')?;
            }
        }
        f.write_char('}')?;
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    // initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "wall_street_wolf=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let _ = dotenv::dotenv();

    let backend = Arc::new(LiveBackend::new().await);

    let watch =
        //scrape::all_stocks_within_price_range(&client, Num::new(3, 1)..Num::new(6, 1)).await;
        scrape::all_top_stocks().await;

    let watch = watch[..watch.len().min(50)].iter().cloned().collect_vec();

    backend.cancel_all_open_orders().await;

    backend.sell_all_positions(|s| !watch.contains(s)).await;

    let mut ticker = Ticker::new(backend.as_ref(), Duration::from_secs_f32(60.0 * 1.5))
        .await
        .unwrap();

    let period = TimePeriod::days(14);

    loop {
        match ticker.wait_for_open_or_tick(backend.as_ref()).await {
            MarketStatus::Open => {
                backend.open().await;

                tracing::debug!("measuring trends...");
                watch_all(
                    backend.as_ref(),
                    watch.clone(),
                    period,
                    30.0..70.0,
                    Duration::from_secs(60 * 30),
                    Num::new(9, 10)..Num::new(15, 10),
                )
                .await;
            }
            MarketStatus::AboutToClose => {
                backend.cancel_all_open_orders().await;

                backend.sell_all_positions(|_| true).await;

                let stats = backend.final_stats().await;

                tracing::info!(
                    "Day ended with ${:.2} equity, an increase of ${:.2} over yesterday",
                    stats.current_equity.to_f64().unwrap(),
                    (stats.current_equity - stats.last_equity).to_f64().unwrap()
                );
            }
        }
    }
}

async fn watch_all<I, S>(
    backend: &(dyn Backend + Sync),
    symbols: I,
    period: TimePeriod,
    rsi_range: std::ops::Range<f64>,
    hold_limit: Duration,
    profit_limit: std::ops::Range<Num>,
) where
    I: IntoIterator<Item = S>,
    S: Into<Symbol>,
{
    let account = backend.account_data();

    // alpaca sorts the latest price data by symbols, alphabetically.
    // it's easier if our list of symbols is already sorted alphabetically,
    // because then we don't have to deal with hashmaps
    let mut symbols = symbols
        .into_iter()
        .map(|s| s.into())
        .filter(|s| {
            // filter out symbols with outstanding orders
            account
                .positions
                .get(s)
                .map_or(true, |pos| !pos.order_in_progress)
        })
        .collect::<Vec<Symbol>>();
    symbols.sort();

    let (all_bars, current_prices) = futures::join!(
        backend.all_latest_bars(symbols.clone(), period, Feed::IEX),
        backend.all_latest_prices(symbols)
    );

    let now = Instant::now();

    for (symbol, bars) in all_bars {
        if bars.is_empty() {
            continue;
        }

        let current_price = current_prices[&symbol].clone();
        let current_price_float = current_price.to_f64().unwrap();
        let bb = bars.bollinger().unwrap();
        let rsi = bars.rsi().unwrap();

        tracing::debug!(
            "{:<5} | (${:.2}) | bb {:.2} < {:.2} < {:.2} | rsi {:.2}",
            symbol,
            current_price_float,
            bb.lower,
            bb.average,
            bb.upper,
            rsi
        );

        let position = account.positions.get(&symbol.clone());

        let all_owned = position
            .as_ref()
            .map(|pos| pos.owned.clone())
            .unwrap_or_default();
        let held_too_long = position
            .as_ref()
            .map_or(false, |pos| now.duration_since(pos.timestamp) > hold_limit);
        let profit_limit_reached =
            position
                .filter(|pos| !pos.buy_in_price.is_zero())
                .map_or(false, |pos| {
                    let profit = current_price / pos.buy_in_price.clone();

                    !profit_limit.contains(&profit)
                });

        if all_owned.is_zero() && rsi < rsi_range.start && current_price_float < bb.lower {
            backend
                .submit_order(symbol, Side::Buy, Amount::quantity(1))
                .await
        } else if !all_owned.is_zero()
            && (held_too_long
                || profit_limit_reached
                || (rsi > rsi_range.end && current_price_float > bb.upper))
        {
            backend
                .submit_order(symbol, Side::Sell, Amount::quantity(all_owned))
                .await
        }
    }
}
