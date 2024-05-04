//! Request/response types for the Washington state contracting portal
//! (WEBS: Washington's Electronic Business Solution)
mod login;
mod home;
mod search_opp;

use {
    crate::{
        httpext::{Form, LogConfig, ResponseExt},
        shapes::{CrawlParameters, NextRequest, Operation, Request, Response},
        soup::parse_html_str,
    },
    lambda_runtime::{Context, Error as LambdaError},
    lazy_static::lazy_static,
    log::*,
    reqwest::Url,
    serde::{Deserialize, Serialize},
    std::{
        fmt::{Display, Formatter, Result as FmtResult},
        str::FromStr,
    },
};

const DEFAULT_WEBS_BASE_URL: &str = "https://pr-webs-vendor.des.wa.gov";
const HOME_PATH: &str = "/Home.aspx";
const LOGIN_PATH: &str = "/LoginPage.aspx";
const SEARCH_BID_PATH: &str = "/Search_Bid.aspx";

pub(crate) const FORM_NAME_FORM1: &str = "Form1";
const FORM_FIELD_EVENTTARGET: &str = "__EVENTTARGET";
const FORM_FIELD_EVENTARGUMENT: &str = "__EVENTARGUMENT";

const OP_START_CRAWL: &str = "StartCrawl";
const OP_FETCH_OPPORTUNITY_LISTING_PAGE: &str = "FetchOpportunityListingPage";
const OP_FETCH_OPPORTUNITY_DETAIL_PAGE: &str = "FetchOpportunityDetailPage";
const OPPORTUNITIES_INITIAL_SIZE: usize = 4096;

lazy_static! {
    static ref DEFAULT_HOME_URL: String = format!("{DEFAULT_WEBS_BASE_URL}{HOME_PATH}");
    static ref DEFAULT_LOGIN_URL: String = format!("{DEFAULT_WEBS_BASE_URL}{LOGIN_PATH}");
}

/// Possible operations for the WEBS service.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum WebsOperation {
    /// Start a crawl on the WEBS service.
    StartCrawl,

    /// Fetch a page of opportunities.
    FetchOpportunityListingPage,

    /// Fetch an opportunity detail page.
    FetchOpportunityDetailPage,
}

/// Encapuslates an event target and event value for a form submission.
#[derive(Clone, Debug)]
pub struct FormEvent {
    /// The target of the event.
    pub target: String,

    /// The value of the event.
    pub argument: String,
}

impl FromStr for WebsOperation {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            OP_START_CRAWL => Ok(WebsOperation::StartCrawl),
            OP_FETCH_OPPORTUNITY_LISTING_PAGE => Ok(WebsOperation::FetchOpportunityListingPage),
            OP_FETCH_OPPORTUNITY_DETAIL_PAGE => Ok(WebsOperation::FetchOpportunityDetailPage),
            _ => Err(format!("Unknown operation: {value}")),
        }
    }
}

impl Display for WebsOperation {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(self.operation())
    }
}

impl WebsOperation {
    /// Handle a request.
    pub async fn handle(self, log_config: LogConfig, req: Request, context: Context) -> Result<Response, LambdaError> {
        match self {
            Self::StartCrawl => start_crawl(log_config, req, context).await,
            Self::FetchOpportunityListingPage => fetch_first_opportunity_listing_page(log_config, req, context).await,
            Self::FetchOpportunityDetailPage => todo!("FetchOpportunityDetailPage not implemented yet"),
        }
    }

    /// Return the operation name within the subsystem.
    pub fn operation(&self) -> &'static str {
        match self {
            Self::StartCrawl => OP_START_CRAWL,
            Self::FetchOpportunityListingPage => OP_FETCH_OPPORTUNITY_LISTING_PAGE,
            Self::FetchOpportunityDetailPage => OP_FETCH_OPPORTUNITY_DETAIL_PAGE,
        }
    }
}

impl FormEvent {
    /// Set the corresponding fields on the form.
    pub fn set_form_fields(&self, form: &mut Form) {
        form.set(FORM_FIELD_EVENTTARGET, &self.target);
        form.set(FORM_FIELD_EVENTARGUMENT, &self.argument);
    }
}

/// Start the WEBS crawl by visiting the login page and submitting credentials.
pub(crate) async fn start_crawl(
    log_config: LogConfig,
    req: Request,
    context: Context,
) -> Result<Response, LambdaError> {
    let url_str = req.url.as_deref().unwrap_or(&DEFAULT_LOGIN_URL);
    let url = Url::parse(url_str)?;

    let client = req.crawl.build_client(log_config.clone(), &context).build()?;

    // Log in to the WEBS portal so we have cookies to identify our session.
    let response = match client.get(url.clone()).send().await {
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
            return Err(e);
        }
    };

    info!("Submitting WEBS login");
    let _ = login::submit_login(&client, &log_config, response).await?;
    info!("WEBS login submitted");

    let cookies = client.cookie_store.read().unwrap().clone();
    let cookie_str = serde_json::to_string(&cookies).unwrap();
    debug!("Cookies: {cookie_str}");

    let start_path = if let Some(host) = url.host() {
        let scheme = url.scheme();
        format!("{scheme}://{host}{HOME_PATH}")
    } else {
        format!("{DEFAULT_WEBS_BASE_URL}{HOME_PATH}")
    };

    let next_op = NextRequest {
        operation: Operation::Webs(WebsOperation::FetchOpportunityListingPage),
        url: Some(start_path),
        crawl: CrawlParameters {
            crawl_id: Some(client.crawl_id),
            user_agent: req.crawl.user_agent,
            cookies,
        },
    };

    Ok(Response {
        next_requests: vec![next_op],
    })
}


/// Visit the home page of the WEBS portal, move to the "Search Opportunities" page, then request the first page of
/// opportunities.
async fn fetch_first_opportunity_listing_page(
    log_config: LogConfig,
    req: Request,
    context: Context,
) -> Result<Response, LambdaError> {
    let url_str = req.url.as_deref().unwrap_or(&DEFAULT_HOME_URL);
    let url = Url::parse(url_str)?;

    let client = req.crawl.build_client(log_config.clone(), &context).build()?;

    // Visit the home page and find the Search Opportunities link.
    let response = match client.get(url.clone()).send().await.error_for_status() {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to fetch WEBS home page: {e}");
            return Err(e);
        }
    };

    let text = match response.text() {
        Ok(t) => t,
        Err(e) => {
            error!("Failed to read WEBS home page: {e}");
            return Err(e.into());
        }
    };

    let search_url = home::find_search_url(&url, text)?;

    // Visit the search opportunities page.
    let response = match client.get(search_url.clone()).send().await.error_for_status() {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to fetch WEBS search opportunities page: {e}");
            return Err(e);
        }
    };

    // Submit the search opportunities link.
    let response = search_opp::submit_search_opps(&client, response).await?;
    let mut next_requests = Vec::with_capacity(OPPORTUNITIES_INITIAL_SIZE);

    // Parse the first page of opportunities.
    let text = response.text()?;
    let document = parse_html_str(text);
    search_opp::parse_opportunity_listing_page(&document, &search_url, &req.crawl, &mut next_requests)?;

    // Parse the form element.
    let form = Form::from_form_name(&search_url, &document, FORM_NAME_FORM1)?;
    
    for form_event in search_opp::find_opportunity_next_pages(&document)? {
        // Visit this search opportunity page by submitting the form with these values.
        let mut form = form.clone();
        form_event.set_form_fields(&mut form);

        let response = match client.request(form.method, form.url).form(&form.fields).send().await.error_for_status() {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to submit WEBS search opportunities form: {e}");
                return Err(e);
            }
        };

        // Parse this page of opportunities.
        let text = response.text()?;
        let document = parse_html_str(text);
        search_opp::parse_opportunity_listing_page(&document, &search_url, &req.crawl, &mut next_requests)?;
    }

    Ok(Response{
        next_requests
    })
}
