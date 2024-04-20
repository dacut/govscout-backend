mod awserr;
mod client;
mod cookie_store;
mod form;
mod logconfig;
mod request;
mod response;

pub use {awserr::*, client::*, cookie_store::*, form::*, logconfig::*, request::*, response::*};

use reqwest::header::{HeaderMap, HeaderValue};

pub const DEFAULT_REDIRECT_LIMIT: usize = 10;
const HEADER_ACCEPT: &str = "Accept";
const DEFAULT_ACCEPT: &str = "text/html,application/xhtml+xml,application/xml;q=0.9*/*;q=0.8";

/// Return the default headers for a request.
pub fn default_headers() -> HeaderMap<HeaderValue> {
    let mut headers = HeaderMap::new();
    headers.insert(HEADER_ACCEPT, HeaderValue::from_static(DEFAULT_ACCEPT));

    headers
}
