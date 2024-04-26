//! Shapes for the request and response types.

use {
    crate::{
        httpext::{default_headers, ClientBuilder, CookieStore, CookieStoreRwLock, LogConfig, DEFAULT_REDIRECT_LIMIT},
        webs::WebsOperation,
    },
    lambda_runtime::{Context, Error as LambdaError},
    log::*,
    reqwest::redirect::Policy as RedirectPolicy,
    serde::{
        de::{Deserializer, Error as SerdeError, Visitor},
        ser::Serializer,
        Deserialize, Serialize,
    },
    std::{
        fmt::{Display, Formatter, Result as FmtResult},
        str::FromStr,
        sync::Arc,
    },
};

/// The default user agent to use when crawling.
pub const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (compatible; GovScout/0.1; +https://github.com/dacut/govscout-backend)";

const SUBSYS_WEBS: &str = "Webs";

/// Operations that can be performed.
#[derive(Clone, Copy, Debug)]
pub enum Operation {
    /// WEBS operation.
    Webs(WebsOperation),
}

/// Request type for all operations.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Request {
    /// The operation to perform.
    pub operation: String,

    /// The URL to start crawling from.
    pub url: Option<String>,

    /// Common crawl parameters
    #[serde(flatten)]
    pub crawl: CrawlParameters,
}

/// Next request to schedule. This is similar to Request but is more strict about types.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct NextRequest {
    /// The operation to perform.
    pub operation: Operation,

    /// The URL to start crawling from.
    pub url: Option<String>,

    /// Common crawl parameters
    #[serde(flatten)]
    pub crawl: CrawlParameters,
}

/// Response type for all operations.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Response {
    /// The next requests to schedule.
    pub next_requests: Vec<NextRequest>,
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

struct OperationVisitor;

impl<'de> Visitor<'de> for OperationVisitor {
    type Value = Operation;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("an operation string")
    }

    fn visit_str<E>(self, value: &str) -> Result<Operation, E>
    where
        E: SerdeError,
    {
        let parts: Vec<&str> = value.split(':').collect();
        if parts.len() != 2 {
            return Err(E::custom("invalid operation format"));
        }

        match parts[0] {
            SUBSYS_WEBS => {
                let webs_op = match WebsOperation::from_str(parts[1]) {
                    Ok(op) => op,
                    Err(_) => return Err(E::custom(format!("Unknown WEBS operation {}", parts[1]))),
                };
                Ok(Operation::Webs(webs_op))
            }
            _ => Err(E::custom("unknown subsystem")),
        }
    }
}

impl<'de> Deserialize<'de> for Operation {
    fn deserialize<D>(deserializer: D) -> Result<Operation, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(OperationVisitor)
    }
}

impl Display for Operation {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            Operation::Webs(op) => write!(f, "{SUBSYS_WEBS}:{op}"),
        }
    }
}

impl FromStr for Operation {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = value.split(':').collect();
        if parts.len() < 2 {
            return Err(format!("Invalid operation format (missing ':'): {value}"));
        }

        match parts[0] {
            SUBSYS_WEBS => Ok(Self::Webs(WebsOperation::from_str(parts[1])?)),
            _ => Err("unknown subsystem".to_string()),
        }
    }

}

impl Operation {
    /// Handle a request.
    pub async fn handle(self, log_config: LogConfig, req: Request, context: Context) -> Result<Response, LambdaError> {
        match self {
            Operation::Webs(op) => op.handle(log_config, req, context).await,
        }
    }

    /// Return the subsystem of the operation.
    pub fn subsystem(&self) -> &'static str {
        match self {
            Operation::Webs(_) => SUBSYS_WEBS,
        }
    }

    /// Return the operation name within the subsystem.
    pub fn operation(&self) -> &'static str {
        match self {
            Operation::Webs(op) => op.operation(),
        }
    }
}

impl Serialize for Operation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_string().as_str())
    }
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

#[cfg(test)]
mod test {
    use crate::{shapes::Operation, webs::WebsOperation};

    /// Check the serialization of operations.
    #[test]
    fn ser_operation() {
        let op = Operation::Webs(WebsOperation::StartCrawl);
        let op = serde_json::to_string(&op).unwrap();
        assert_eq!(op.as_str(), r#""Webs:StartCrawl""#);
    }
}
