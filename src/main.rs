use apca::{
    api::v2::{
        clock,
        order::{self, Side, Type},
    },
    data::v2::{quotes, Feed},
    *,
};
use chrono::{DateTime, Days, Duration, TimeZone, Utc};
use tracing_subscriber::{prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt};

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

    let clock = client.issue::<clock::Get>(&()).await.unwrap();
    if !clock.open {
        tracing::info!(
            "The market opens on {} - {}",
            clock.next_open.format("%A %d/%m/%Y at %I:%M %P"),
            clock.next_close.format("%I:%M %P")
        );
    } else {
        tracing::info!(
            "The market closes at {}",
            clock.next_close.format("%I:%M %P")
        )
    }

    // let request = order::OrderReqInit {
    //     type_: Type::Market,
    //     ..Default::default()
    // }
    // .init("AAPL", Side::Buy, order::Amount::quantity(1));

    // let order = client.issue::<order::Post>(&request).await.unwrap();

    fn now() -> DateTime<Utc> {
        let now = Utc::now();
        now.checked_sub_days(Days::new(1)).unwrap()
    }

    let to = now().checked_sub_signed(Duration::minutes(1)).unwrap();
    let from = to.checked_sub_signed(Duration::minutes(29)).unwrap();

    let request = quotes::QuotesReqInit {
        // feed: Some(Feed::IEX),
        ..Default::default()
    }
    .init("AAPL", from, to);

    let quote = client.issue::<quotes::Get>(&request).await.unwrap();
    if quote.next_page_token.is_some() {
        tracing::error!("more pages than expected");
    }

    let mut counts = [0; 6];
    let mut sums = [0.0; 6];

    for quote in quote.quotes {
        let idx = quote.time.signed_duration_since(from).num_minutes() / 5;
        counts[idx as usize] += 1;
        sums[idx as usize] += quote.bid_price.to_f64().unwrap();
        tracing::info!(
            "#{} {} - bid {}@{} ask {}@{}",
            idx,
            quote.time.format("%I:%M %P"),
            quote.bid_size,
            quote.bid_price,
            quote.ask_size,
            quote.ask_price
        );
    }

    let mut averages = [0.0; 6];

    for idx in 0..sums.len() {
        averages[idx] = sums[idx] / counts[idx] as f64;
    }

    tracing::info!("averages {:#?}", averages);
}
