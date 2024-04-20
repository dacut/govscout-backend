//! Request/response types for the Washington state contracting portal
//! (WEBS: Washington's Electronic Business Solution)
use {
    crate::{
        httpext::{Client, Form, LogConfig, Response},
        shapes::CrawlParameters,
        BoxError,
    },
    lambda_runtime::{Context, Error},
    log::*,
    reqwest::Url,
    serde::{Deserialize, Serialize},
};

const DEFAULT_START_URL: &str = "https://pr-webs-vendor.des.wa.gov/LoginPage.aspx";
const SEARCH_START_PATH: &str = "https://pr-webs-vendor.des.wa.gov/Search_Bid.aspx";
const SSM_WEBS_USERNAME_PARAM: &str = "Webs/Username";
const SSM_WEBS_PASSWORD_PARAM: &str = "Webs/Password";
const WEBS_TXT_EMAIL_PARAM: &str = "txtEmail";
const WEBS_TXT_PASSWORD_PARAM: &str = "txtPassword";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct StartWebsCrawlRequest {
    /// Common crawl parameters
    #[serde(flatten)]
    pub crawl: CrawlParameters,

    /// The URL to start crawling from.
    pub url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct StartWebsCrawlResponse {
    /// Common crawl parameters
    #[serde(flatten)]
    pub crawl_parameters: CrawlParameters,

    /// The next operation to perform. Should be "CrawlWebsOpportunityList".
    pub next_operation: String,

    /// The URL to start crawling from.
    pub url: String,
}

pub(crate) async fn start_webs_crawl(
    log_config: LogConfig,
    req: StartWebsCrawlRequest,
    context: Context,
) -> Result<StartWebsCrawlResponse, Error> {
    let url_str = req.url.as_ref().map(String::as_str).unwrap_or(DEFAULT_START_URL);
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

    Ok(StartWebsCrawlResponse {
        crawl_parameters: CrawlParameters {
            crawl_id: Some(client.crawl_id),
            user_agent: req.crawl.user_agent,
            cookies,
        },
        next_operation: "CrawlWebsOpportunityList".to_string(),
        url: SEARCH_START_PATH.to_string(),
    })
}

/// Submit the login form to the WEBS portal.
async fn submit_login(client: &Client, log_config: &LogConfig, response: Response) -> Result<Response, BoxError> {
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
