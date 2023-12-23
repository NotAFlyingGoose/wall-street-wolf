use std::{fs, time::Duration};

use futures::future::join_all;
use itertools::Itertools;
use lazy_static::lazy_static;
use num_decimal::Num;
use scraper::{Html, Selector};
use tokio::time::Instant;

use crate::{backend::Backend, Symbol};

const YAHOO_FINANCE: &str = "https://finance.yahoo.com/";
const MARKET_WATCH: &str = "https://www.marketwatch.com/investing";
const SLICK_CHARTS: &str = "https://www.slickcharts.com/sp500";
const INVESTOPEDIA_TOP_STOCKS: &str = "https://www.investopedia.com/top-stocks-june-2023-7505936";

lazy_static! {
    static ref CLIENT: reqwest::Client = reqwest::Client::builder().build().unwrap();
}

pub(crate) async fn all_stocks_within_price_range(
    backend: &dyn Backend,
    price_range: std::ops::Range<Num>,
) -> Vec<(Symbol, Num)> {
    let all_assets = backend.all_active_assets().await;

    let mut results = Vec::with_capacity(all_assets.len());

    let mut last_sleep = Instant::now();

    // we can't just call `get_latest_prices` with ALL the assets because the url will get too long
    for (idx, assets) in all_assets.into_iter().chunks(1000).into_iter().enumerate() {
        let latest_prices = backend
            .all_latest_prices(assets.collect())
            .await
            .into_iter()
            .filter(|(_, price)| price_range.contains(price));

        results.extend(latest_prices);

        if idx % 150 == 149 && last_sleep.elapsed().as_secs() < 60 {
            tracing::debug!("sleeping for rate limit");
            tokio::time::sleep(Duration::from_secs(60)).await;
            last_sleep = Instant::now();
        }
    }

    results.shrink_to_fit();

    results
}

pub(crate) async fn all_top_stocks() -> Vec<Symbol> {
    let (sp_500, top_stocks) = futures::join!(sp_500(), investopedia_top_stocks());
    sp_500
        .iter()
        .chain(top_stocks.iter())
        .unique()
        .map(Symbol::from)
        .collect()
}

pub(crate) async fn investopedia_top_stocks() -> Vec<String> {
    let body = &CLIENT
        .get(INVESTOPEDIA_TOP_STOCKS)
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let doc = Html::parse_document(body);

    let sel = Selector::parse("tbody").unwrap();

    doc.select(&sel)
        .flat_map(|tbody| {
            tbody.children().filter_map(|tr| {
                tr.children()
                    .nth(1)? // <td>
                    .children()
                    .nth(1)? // <a>
                    .children()
                    .next()? // text
                    .value()
                    .as_text()
                    .map(|text| text.to_string())
            })
        })
        .unique()
        .collect()
}

pub(crate) async fn sp_500() -> Vec<String> {
    let body = &CLIENT
        .get(SLICK_CHARTS)
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let doc = Html::parse_document(body);

    let sel = Selector::parse("tbody").unwrap();

    doc.select(&sel)
        .next()
        .unwrap()
        .children()
        .filter_map(|tr| {
            tr.children()
                .nth(5)? // <td>
                .children()
                .next()? // <a>
                .children()
                .next()? // text
                .value()
                .as_text()
                .map(|text| text.to_string())
        })
        .collect()
}

pub(crate) async fn scrape_news() -> Vec<String> {
    let body = &CLIENT
        .get(MARKET_WATCH)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let doc = Html::parse_document(body);

    let sel = Selector::parse("a").unwrap();

    let stocks = doc
        .select(&sel)
        .filter_map(|el| {
            el.value().attr("href").filter(|link| {
                !link.is_empty() && (link.contains("/articles/") || link.contains("/story/"))
            })
        })
        .unique()
        .map(scrape_article)
        .collect::<Vec<_>>();

    join_all(stocks).await;

    Vec::new()
}

async fn scrape_article(link: &str) -> Option<(String, f32)> {
    let body = &CLIENT
        .get(MARKET_WATCH)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    use std::io::Write;

    let filename = format!(
        "news/{}.html",
        link.split('/').last().unwrap().split('?').next().unwrap()
    );
    let _ = fs::create_dir("news");
    tracing::info!("saving response in {}", filename);
    let mut file = fs::File::create(filename).unwrap();
    let _ = writeln!(file, "{}", body);

    let doc = Html::parse_document(body);

    let sel = Selector::parse(".list--tickers").unwrap();
    let referenced = doc
        .select(&sel)
        .next()?
        .children()
        .map(|el| {
            el.children()
                .next()
                .unwrap()
                .value()
                .as_text()
                .unwrap()
                .to_string()
        })
        .collect::<Vec<_>>();

    tracing::info!("{} - {:?}", link, referenced);

    Some((String::new(), 0.0))
}

mod tests {
    // #[tokio::test]
    // async fn sp_500_is_500() {
    //     let top = sp_500().await;
    //     assert_eq!(top.len(), 500);
    // }

    // #[tokio::test]
    // async fn test() {
    //     let top = investopedia_top_stocks().await;
    //     println!("{:#?}", top);
    //     assert!(false);
    // }
}
