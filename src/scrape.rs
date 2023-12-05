use std::fs;

use futures::future::join_all;
use itertools::Itertools;
use lazy_static::lazy_static;
use scraper::{ElementRef, Html, Selector};

const YAHOO_FINANCE: &str = "https://finance.yahoo.com/";
const MARKET_WATCH: &str = "https://www.marketwatch.com/investing";
const SLICK_CHARTS: &str = "https://www.slickcharts.com/sp500";
const INVESTOPEDIA_TOP_STOCKS: &str = "https://www.investopedia.com/top-stocks-june-2023-7505936";

lazy_static! {
    static ref CLIENT: reqwest::Client = reqwest::Client::builder().build().unwrap();
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
        .map(|tbody| {
            tbody.children().filter_map(|tr| {
                tr.children()
                    .skip(1)
                    .next()? // <td>
                    .children()
                    .skip(1)
                    .next()? // <a>
                    .children()
                    .next()? // text
                    .value()
                    .as_text()
                    .map(|text| text.to_string())
            })
        })
        .flatten()
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
                .skip(5)
                .next()? // <td>
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
        link.split("/").last().unwrap().split("?").next().unwrap()
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
    use super::*;

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
