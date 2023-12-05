use apca::{
    api::v2::clock::{self, Clock},
    Client,
};
use chrono::{DateTime, Local, Utc};
use tokio::time::Interval;

pub(crate) enum MarketStatus {
    Open,
    AboutToClose,
}

pub(crate) async fn wait_for_open_or_tick(
    client: &Client,
    clock: &mut Clock,
    interval: &mut Interval,
) -> MarketStatus {
    let about_to_close = clock
        .next_close
        .signed_duration_since(Utc::now())
        .to_std()
        .unwrap()
        <= interval.period();
    if clock.open {
        if about_to_close {
            return MarketStatus::AboutToClose;
        }

        interval.tick().await;
        return MarketStatus::Open;
    }

    // if the market is closed, wait for it to open and then get the next clock's information

    let next_open: DateTime<Local> = DateTime::from(clock.next_open);
    let next_close: DateTime<Local> = DateTime::from(clock.next_close);

    tracing::info!(
        "Sleeping until the market opens on {} - {}",
        next_open.format("%A %d/%m/%Y at %I:%M %P"),
        next_close.format("%I:%M %P")
    );

    tokio::time::sleep(
        clock
            .next_open
            .signed_duration_since(Utc::now())
            .to_std()
            .unwrap(),
    )
    .await;

    // replace the clock with today's info
    *clock = client.issue::<clock::Get>(&()).await.unwrap();

    assert!(clock.open);

    tracing::info!("Sleep over");

    MarketStatus::Open
}
