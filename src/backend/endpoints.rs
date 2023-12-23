use std::collections::BTreeMap;

use apca::data::v2::Feed;
use chrono::{DateTime, Utc};
use http::Method;
use num_decimal::Num;
use serde::{Deserialize, Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub(crate) enum ConversionError {
    /// A variant used when a JSON conversion failed.
    #[error("failed to convert from/to JSON")]
    Json(#[from] serde_json::Error),
    /// A variant used when we fail to URL-encode a piece of data.
    #[error("failed to URL-encode data")]
    UrlEncode(#[from] serde_urlencoded::ser::Error),
}

const DATA_BASE_URL: &str = "https://data.alpaca.markets";

#[derive(Debug, serde::Serialize)]
pub(crate) struct CryptoTradesReq {
    #[serde(skip)]
    symbol: String,
    /// The maximum number of trades to be returned for each symbol.
    ///
    /// It can be between 1 and 10000. Defaults to 1000 if the provided
    /// value is None.
    #[serde(rename = "limit")]
    pub limit: Option<usize>,
    /// Filter trades equal to or after this time.
    #[serde(rename = "start")]
    pub start: DateTime<Utc>,
    /// Filter trades equal to or before this time.
    #[serde(rename = "end")]
    pub end: DateTime<Utc>,
    /// The data feed to use.
    ///
    /// Defaults to [`IEX`][Feed::IEX] for free users and
    /// [`SIP`][Feed::SIP] for users with an unlimited subscription.
    #[serde(rename = "feed")]
    pub feed: String,
    /// If provided we will pass a page token to continue where we left off.
    #[serde(rename = "page_token", skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
}

/// A market data trade as returned by the /v2/stocks/{symbol}/trades endpoint.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) struct CryptoTrade {
    /// Time of the trade.
    #[serde(rename = "t")]
    pub timestamp: DateTime<Utc>,
    /// The price of the trade.
    #[serde(rename = "p")]
    pub price: Num,
    /// The size of the trade.
    #[serde(rename = "s")]
    pub size: usize,
}

/// A collection of trades as returned by the API. This is one page of trades.
#[derive(Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) struct CryptoTrades {
    /// The list of returned trades.
    #[serde(deserialize_with = "vec_from_str")]
    pub trades: Vec<CryptoTrade>,
    /// The symbol the trades correspond to.
    pub symbol: String,
    /// The token to provide to a request to get the next page of trades for this request.
    pub next_page_token: Option<String>,
}

http_endpoint::EndpointDef! {
    pub(crate) GetCryptoTrades(CryptoTradesReq),

    Ok => CryptoTrades, [
        /* 200 */ OK,
    ],
    Err => GetCryptoTradesErr, [
        NOT_FOUND => NotFound,
        BAD_REQUEST => InvalidInput,
        FORBIDDEN => NotPermitted,
        TOO_MANY_REQUESTS => RateLimitExceeded,
    ],
    ConversionErr => ConversionError,
    ApiErr => apca::ApiError,

    fn base_url() -> Option<http_endpoint::Str> {
        Some(DATA_BASE_URL.into())
    }

    fn path(input: &Self::Input) -> http_endpoint::Str {
        format!("/v2/stocks/{}/trades", input.symbol).into()
    }

    fn parse(body: &[u8]) -> Result<Self::Output, Self::ConversionError> {
        let txt = std::str::from_utf8(body).unwrap();
        std::fs::write("response.json", txt).unwrap();
        serde_json::from_slice::<Self::Output>(body).map_err(Self::ConversionError::from)
    }

    fn parse_err(body: &[u8]) -> Result<Self::ApiError, Vec<u8>> {
        serde_json::from_slice::<Self::ApiError>(body).map_err(|_| body.to_vec())
    }

    fn query(input: &Self::Input) -> Result<Option<http_endpoint::Str>, Self::ConversionError> {
        Ok(Some(serde_urlencoded::to_string(input)?.into()))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct CancelledOrder {
    id: String,
    status: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct AllCancelledOrders(pub(crate) Vec<CancelledOrder>);

http_endpoint::EndpointDef! {
    pub(crate) CancelAllOrders(()),

    Ok => AllCancelledOrders, [
        /* 207 */ MULTI_STATUS,
    ],
    Err => CancelAllOrdersErr, [
        NOT_FOUND => NotFound,
        BAD_REQUEST => InvalidInput,
        FORBIDDEN => NotPermitted,
        TOO_MANY_REQUESTS => RateLimitExceeded,
        INTERNAL_SERVER_ERROR => InternalServerError, // the delete failed
    ],
    ConversionErr => ConversionError,
    ApiErr => apca::ApiError,

    #[inline]
    fn method() -> Method {
        Method::DELETE
    }

    fn path(_: &Self::Input) -> http_endpoint::Str {
        "/v2/orders".into()
    }

    fn parse(body: &[u8]) -> Result<Self::Output, Self::ConversionError> {
        let txt = std::str::from_utf8(body).unwrap();
        std::fs::write("response.json", txt).unwrap();
        serde_json::from_slice::<Self::Output>(body).map_err(Self::ConversionError::from)
    }

    fn parse_err(body: &[u8]) -> Result<Self::ApiError, Vec<u8>> {
        serde_json::from_slice::<Self::ApiError>(body).map_err(|_| body.to_vec())
    }
}

/// A GET request to be made to the /v2/stocks/quotes/latest endpoint.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LastTradesReq {
    /// The symbols to retrieve the last quote for.
    #[serde(rename = "symbols", serialize_with = "string_slice_to_str")]
    pub symbols: Vec<String>,
    /// The data feed to use.
    #[serde(rename = "feed")]
    pub feed: Option<Feed>,
}

/// A helper for initializing [`LastQuotesReq`] objects.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[allow(missing_copy_implementations)]
pub struct LastTradesReqInit {
    /// See `LastQuotesReq::feed`.
    pub feed: Option<Feed>,
    #[doc(hidden)]
    pub _non_exhaustive: (),
}

impl LastTradesReqInit {
    /// Create a [`LastQuotesReq`] from a `LastQuotesReqInit`.
    #[inline]
    pub fn init<I, S>(self, symbols: I) -> LastTradesReq
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        LastTradesReq {
            symbols: symbols.into_iter().map(S::into).collect(),
            feed: self.feed,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[non_exhaustive]
pub struct LastTrade {
    /// Time of the trade.
    #[serde(rename = "t")]
    pub timestamp: DateTime<Utc>,
    /// The price of the trade.
    #[serde(rename = "p")]
    pub price: Num,
    /// The size of the trade.
    #[serde(rename = "s")]
    pub size: usize,
}

http_endpoint::EndpointDef! {
    pub(crate) GetLastTrades(LastTradesReq),

    Ok => Vec<(String, LastTrade)>, [
        /* 200 */ OK,
    ],
    Err => GetLatestTradesErr, [
        NOT_FOUND => NotFound,
        BAD_REQUEST => InvalidInput,
        FORBIDDEN => NotPermitted,
        TOO_MANY_REQUESTS => RateLimitExceeded,
    ],
    ConversionErr => ConversionError,
    ApiErr => apca::ApiError,

    fn base_url() -> Option<http_endpoint::Str> {
        Some(DATA_BASE_URL.into())
    }

    fn path(_: &Self::Input) -> http_endpoint::Str {
        "/v2/stocks/trades/latest".into()
    }

    fn query(input: &Self::Input) -> Result<Option<http_endpoint::Str>, Self::ConversionError> {
    Ok(Some(serde_urlencoded::to_string(input)?.into()))
  }

  fn parse(body: &[u8]) -> Result<Self::Output, Self::ConversionError> {
    // TODO: Ideally we'd write our own deserialize implementation here
    //       to create a vector right away instead of going through a
    //       BTreeMap.

    /// A helper object for parsing the response to a `Get` request.
    #[derive(Deserialize)]
    struct Response {
      /// A mapping from symbols to quote objects.
      // We use a `BTreeMap` here to have a consistent ordering of
      // trades.
      trades: BTreeMap<String, LastTrade>,
    }

    // We are not interested in the actual `Response` object. Clients
    // can keep track of what symbol they requested a quote for.
    serde_json::from_slice::<Response>(body)
      .map(|response| {
        response
          .trades
          .into_iter()
          .collect()
      })
      .map_err(Self::ConversionError::from)
  }

    fn parse_err(body: &[u8]) -> Result<Self::ApiError, Vec<u8>> {
        serde_json::from_slice::<Self::ApiError>(body).map_err(|_| body.to_vec())
    }
}

/// Deserialize a `Vec` from a string that could contain a `null`.
pub(crate) fn vec_from_str<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    let vec = <Option<Vec<T>> as serde::Deserialize>::deserialize(deserializer)?;
    Ok(vec.unwrap_or_default())
}

/// Serialize a slice into a string of textual representations of the
/// elements, retrieved by applying a function to each, and then
/// separated by comma.
pub(crate) fn slice_to_str<S, F, T>(
    slice: &[T],
    name_fn: F,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    F: Fn(&T) -> http_endpoint::Str,
    T: Serialize,
{
    if !slice.is_empty() {
        // `serde_urlencoded` seemingly does not know how to handle a
        // `Vec`. So what we do is we convert each and every element to a
        // string and then concatenate them, separating each by comma.
        let s = slice.iter().map(name_fn).collect::<Vec<_>>().join(",");
        serializer.serialize_str(&s)
    } else {
        serializer.serialize_none()
    }
}

/// Serialize a slice of strings into a comma-separated string combining
/// the individual strings.
pub(crate) fn string_slice_to_str<S>(slice: &[String], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    #[allow(clippy::ptr_arg)]
    fn name_fn(string: &String) -> http_endpoint::Str {
        string.clone().into()
    }

    slice_to_str(slice, name_fn, serializer)
}
