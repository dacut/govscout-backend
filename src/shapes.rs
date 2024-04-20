//! Shapes for the request and response types.

use {
    crate::{
        httpext::{default_headers, ClientBuilder, CookieStore, CookieStoreRwLock, LogConfig, DEFAULT_REDIRECT_LIMIT},
        webs::{StartWebsCrawlRequest, StartWebsCrawlResponse},
    },
    lambda_runtime::Context,
    log::*,
    reqwest::redirect::Policy as RedirectPolicy,
    serde::{Deserialize, Serialize},
    std::sync::Arc,
};

pub(crate) const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (compatible; GovScout/0.1; +https://github.com/dacut/govscout-backend)";

/// Union of all request types.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "Operation", content = "Request")]
pub enum Request {
    StartWebsCrawl(StartWebsCrawlRequest),
}

/// Union of all response types.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Response {
    StartWebsCrawl(StartWebsCrawlResponse),
}

impl From<StartWebsCrawlResponse> for Response {
    fn from(resp: StartWebsCrawlResponse) -> Self {
        Response::StartWebsCrawl(resp)
    }
}

/// Common parameters for crawling.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct CrawlParameters {
    /// The unique ID for this crawl.
    pub crawl_id: Option<String>,

    /// User agent to use for the crawl.
    #[serde(default = "default_user_agent")]
    pub user_agent: String,

    /// Cookies to use for the crawl.
    #[serde(default)]
    pub cookies: CookieStore,
}

#[inline]
pub fn default_user_agent() -> String {
    DEFAULT_USER_AGENT.to_string()
}

impl CrawlParameters {
    /// Create a new Reqwest [ClientBuilder] with the appropriate settings from the crawl parameters.
    pub fn build_client(&self, log_config: LogConfig, context: &Context) -> ClientBuilder {
        let cookie_store = Arc::new(CookieStoreRwLock::from(self.cookies.clone()));

        let crawl_id = match self.crawl_id.as_ref() {
            Some(crawl_id) => crawl_id.clone(),
            None => {
                info!("Assigning new crawl ID from Lambda context: {}", context.request_id);
                context.request_id.clone()
            }
        };

        let builder = reqwest::ClientBuilder::new()
            .user_agent(self.user_agent.as_str())
            .default_headers(default_headers())
            .cookie_provider(cookie_store.clone())
            .deflate(true)
            .gzip(true)
            .brotli(true)
            .redirect(RedirectPolicy::limited(DEFAULT_REDIRECT_LIMIT));

        ClientBuilder {
            builder,
            log_config: Some(log_config),
            crawl_id,
            cookie_store,
        }
    }
}
