mod prices;
mod scrape;
mod wait;

use std::{cmp::Ordering, collections::HashMap, time::Duration};

use apca::{
    api::v2::{
        account,
        clock::{self},
        order::{self, Amount, Side},
        position, positions,
    },
    *,
};
use itertools::Itertools;
use tracing_subscriber::{prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt};

use crate::wait::MarketStatus;

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

    if dotenv::dotenv().is_ok() {
        tracing::debug!("found .env");
    }

    let api_info = ApiInfo::from_env().unwrap();
    let client = Client::new(api_info);

    let zion_moving_average = prices::get_all_moving_averages(&client, ["ZION"], 10).await[0];
    let zion_price = prices::get_latest_prices(&client, ["ZION"]).await[0];

    // tracing::info!("ZION moving avg: {}", zion_moving_average);
    // tracing::info!("ZION price     : {}", zion_price);
    // tracing::info!("ZION trend     : {}", zion_price / zion_moving_average);

    // return;

    let mut positions = client
        .issue::<positions::Get>(&())
        .await
        .unwrap()
        .into_iter()
        .map(|positon| (positon.symbol, positon.quantity.to_i64().unwrap()))
        .collect::<HashMap<_, _>>();

    tracing::info!("positions: {:#?}", positions);

    let sp_500 = scrape::sp_500().await;
    let top_stocks = scrape::investopedia_top_stocks().await;
    let watch = sp_500[450..500]
        .iter()
        .cloned()
        .chain(top_stocks.into_iter())
        .unique()
        .collect::<Vec<_>>();

    for (symbol, owned) in positions.clone() {
        if !watch.contains(&&symbol) {
            order(&client, &mut positions, symbol, Side::Sell, owned).await;
        }
    }

    // wait for two minutes
    let mut interval = tokio::time::interval(Duration::from_secs_f32(60.0 * 1.5));

    let mut clock = client.issue::<clock::Get>(&()).await.unwrap();

    loop {
        match wait::wait_for_open_or_tick(&client, &mut clock, &mut interval).await {
            MarketStatus::Open => {
                tracing::debug!("measuring trends...");
                watch_all(&client, &mut positions, &watch, 1.001).await;
            }
            MarketStatus::AboutToClose => {
                for (symbol, owned) in positions.clone() {
                    order(&client, &mut positions, symbol, Side::Sell, owned).await
                }

                let account = client.issue::<account::Get>(&()).await.unwrap();

                tracing::info!(
                    "Day ended with ${} equity, an increase of ${} over yesterday",
                    account.equity.to_f64().unwrap(),
                    account.equity.to_f64().unwrap() - account.last_equity.to_f64().unwrap()
                );

                // final tick of the day
                interval.tick().await;
            }
        }
    }
}

async fn watch_all<I, S>(
    client: &Client,
    positions: &mut HashMap<String, i64>,
    symbols: I,
    buy_sell_cutoff: f64,
) where
    I: IntoIterator<Item = S> + Clone,
    S: Into<String> + Clone + Ord,
{
    // alpaca sorts the latest price data by symbols, alphabetically.
    // it's easier if our list of symbols is already sorted alphabetically,
    // because then we don't have to deal with hashmaps
    let mut symbols = symbols.into_iter().collect::<Vec<_>>();
    symbols.sort();

    let averages = prices::get_all_moving_averages(&client, symbols.clone(), 10).await;
    let latest = prices::get_latest_prices(&client, symbols.clone()).await;

    let trends = symbols.into_iter().zip(
        averages
            .into_iter()
            .zip(latest.into_iter())
            .map(|(average, latest)| latest / average)
            .filter(|trend| !trend.is_nan()),
    );

    for (symbol, trend) in trends {
        tracing::debug!("{} - {}", symbol.clone().into(), trend);
        // buy if the trend is positive (>1), sell if the trend is negative (<1)
        match trend.total_cmp(&buy_sell_cutoff) {
            std::cmp::Ordering::Less => {
                let owned = *positions.get(&symbol.clone().into()).unwrap_or(&0);

                if owned > 0 {
                    order(&client, positions, symbol, Side::Sell, owned).await
                }
            }
            std::cmp::Ordering::Equal => {}
            std::cmp::Ordering::Greater => {
                order(
                    &client,
                    positions,
                    symbol,
                    Side::Buy,
                    (trend as f64 * 1.5) as i64,
                )
                .await
            }
        }
    }
}

async fn order<S>(
    client: &Client,
    positions: &mut HashMap<String, i64>,
    symbol: S,
    side: Side,
    amount: i64,
) where
    S: Into<String> + Clone,
{
    let request = order::OrderReqInit {
        ..Default::default()
    }
    .init(symbol.clone(), side, Amount::quantity(amount));

    let _ = client.issue::<order::Post>(&request).await.unwrap();

    match side {
        Side::Buy => {
            tracing::debug!("Buying {} of {}", amount, symbol.clone().into());
            positions
                .entry(symbol.into())
                .and_modify(|owned| *owned += amount)
                .or_insert(amount);
        }
        Side::Sell => {
            tracing::debug!("Selling {} of {}", amount, symbol.clone().into());

            let owned = positions[&symbol.clone().into()];

            match owned.cmp(&amount) {
                Ordering::Less => unreachable!("Sold more stock than owned"),
                Ordering::Equal => {
                    positions.remove(&symbol.into());
                }
                Ordering::Greater => {
                    positions
                        .entry(symbol.into())
                        .and_modify(|owned| *owned -= amount);
                }
            }
        }
    }
}
