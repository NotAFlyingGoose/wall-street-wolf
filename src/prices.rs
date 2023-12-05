use std::collections::HashMap;

use apca::{
    data::v2::{last_quotes, trades, Feed},
    Client,
};
use chrono::Utc;

pub(crate) async fn get_latest_prices<S, I>(client: &Client, symbols: I) -> Vec<f64>
where
    S: Into<String> + Clone,
    I: IntoIterator<Item = S> + Clone,
{
    let request = last_quotes::LastQuotesReqInit {
        // feed: Some(Feed::IEX),
        ..Default::default()
    }
    .init(symbols.clone());

    let data = client.issue::<last_quotes::Get>(&request).await.unwrap();

    let (returned_symbols, quotes): (Vec<_>, Vec<_>) = data.into_iter().unzip();

    assert_eq!(
        symbols.into_iter().map(Into::into).collect::<Vec<_>>(),
        returned_symbols
    );

    quotes
        .into_iter()
        .map(|quote| quote.ask_price.to_f64().unwrap())
        .collect()
}

pub(crate) async fn get_all_moving_averages<I, S>(
    client: &Client,
    symbols: I,
    minutes: i64,
) -> Vec<f64>
where
    S: Into<String> + Clone,
    I: IntoIterator<Item = S>,
{
    let mut averages = Vec::new();
    for symbol in symbols.into_iter() {
        averages.push(get_moving_average(&client, symbol, minutes));
    }
    futures::future::join_all(averages).await
}

async fn get_moving_average<S>(client: &Client, symbol: S, minutes: i64) -> f64
where
    S: Into<String>,
{
    let to = Utc::now()
        .checked_sub_signed(chrono::Duration::minutes(1))
        .unwrap();
    let from = to
        .checked_sub_signed(chrono::Duration::minutes(minutes - 1))
        .unwrap();

    let request = trades::TradesReqInit {
        feed: Some(Feed::IEX),
        ..Default::default()
    }
    .init(symbol, from, to);

    let data = client.issue::<trades::Get>(&request).await.unwrap();
    if data.next_page_token.is_some() {
        tracing::error!("more pages than expected");
    }

    // calculate the average of all the trades
    data.trades
        .iter()
        .map(|trade| trade.price.to_f64().unwrap())
        .sum::<f64>()
        / data.trades.len() as f64
}
