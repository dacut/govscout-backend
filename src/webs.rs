//! Request/response types for the Washington state contracting portal
//! (WEBS: Washington's Electronic Business Solution)
use {
    crate::{
        httpext::{Client, Form, LogConfig, Response as HttpResponse},
        shapes::{CrawlParameters, Request, Response},
        BoxError,
    },
    lambda_runtime::{Context, Error},
    log::*,
    reqwest::Url,
    serde::{Deserialize, Serialize},
    std::{
        fmt::{Display, Formatter, Result as FmtResult},
        str::FromStr,
    },
};

const DEFAULT_START_URL: &str = "https://pr-webs-vendor.des.wa.gov/LoginPage.aspx";
const SEARCH_START_PATH: &str = "https://pr-webs-vendor.des.wa.gov/Search_Bid.aspx";
const SSM_WEBS_USERNAME_PARAM: &str = "Webs/Username";
const SSM_WEBS_PASSWORD_PARAM: &str = "Webs/Password";
const WEBS_TXT_EMAIL_PARAM: &str = "txtEmail";
const WEBS_TXT_PASSWORD_PARAM: &str = "txtPassword";
const OP_START_CRAWL: &str = "StartCrawl";

/// Possible operations for the WEBS service.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum WebsOperation {
    /// Start a crawl on the WEBS service.
    StartCrawl,
}

impl FromStr for WebsOperation {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            OP_START_CRAWL => Ok(WebsOperation::StartCrawl),
            _ => Err(()),
        }
    }
}

impl Display for WebsOperation {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            WebsOperation::StartCrawl => f.write_str(OP_START_CRAWL),
        }
    }
}

pub(crate) async fn start_crawl(log_config: LogConfig, req: Request, context: Context) -> Result<Response, Error> {
    let url_str = req.url.as_deref().unwrap_or(DEFAULT_START_URL);
    let url = Url::parse(url_str)?;

    let client = req.crawl.build_client(log_config.clone(), &context).build()?;

    // Log in to the WEBS portal so we have cookies to identify our session.
    let response = match client.get(url).send().await {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to fetch WEBS login page: {e}");
            return Err(e);
        }
    };

    let response = match response.error_for_status() {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to fetch WEBS login page: {e}");
            return Err(e.into());
        }
    };

    info!("Submitting WEBS login");

    let _ = submit_login(&client, &log_config, response).await?;

    info!("WEBS login submitted");

    let cookies = client.cookie_store.read().unwrap().clone();

    let cookie_str = serde_json::to_string(&cookies).unwrap();
    debug!("Cookies: {cookie_str}");

    let next_op = Request {
        operation: "CrawlWebsOpportunityList".to_string(),
        url: Some(SEARCH_START_PATH.to_string()),
        crawl: CrawlParameters {
            crawl_id: Some(client.crawl_id),
            user_agent: req.crawl.user_agent,
            cookies,
        },
    };

    Ok(Response {
        next_operations: vec![next_op],
    })
}

/// Submit the login form to the WEBS portal.
async fn submit_login(
    client: &Client,
    log_config: &LogConfig,
    response: HttpResponse,
) -> Result<HttpResponse, BoxError> {
    let url = response.url().clone();
    debug!("WEBS login form URL: {url}");

    let text = match response.text() {
        Ok(t) => t,
        Err(e) => {
            error!("Failed to read WEBS login form: {e}");
            return Err(e.into());
        }
    };

    debug!("WEBS login form text has {} bytes", text.len());

    let mut form = match Form::from_unparsed_form_name(&url, text, "Form1") {
        Ok(form) => form,
        Err(e) => {
            error!("Failed to parse WEBS login form: {e}");
            return Err(e);
        }
    };

    let username = log_config.get_parameter(SSM_WEBS_USERNAME_PARAM).await?;
    let password = log_config.get_parameter(SSM_WEBS_PASSWORD_PARAM).await?;

    form.set(WEBS_TXT_EMAIL_PARAM, username);
    form.set(WEBS_TXT_PASSWORD_PARAM, password);

    let response = match client.request(form.method, form.url).form(&form.fields).send().await {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to submit WEBS login form: {e}");
            return Err(e);
        }
    };
    let response = response.error_for_status()?;
    Ok(response)
}
