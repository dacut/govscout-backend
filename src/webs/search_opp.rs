//! WEBS search opportunities page handling.
use {
    crate::{
        httpext::{Client, Form, Response as HttpResponse, ResponseExt},
        shapes::{CrawlParameters, NextRequest, Operation},
        soup::{NodeExt, QueryBuilderExt},
        webs::{FormEvent, WebsOperation},
        BoxError,
    },
    log::*,
    markup5ever_rcdom::RcDom,
    reqwest::Url,
};

const WEBS_RAD_COMM_CODES_PARAM: &str = "radCommCodes";
const WEBS_RAD_COUNTIES_PARAM: &str = "radCounties";

const WEBS_CLASS_GRID3FILE1: &str = "Grid3File1";
const WEBS_CLASS_GRID3FILE2: &str = "Grid3File2";
const WEBS_CLASS_GRID3PAGER: &str = "Grid3Pager";
const WEBS_OPPORTUNITY_CLASSES: &[&str] = &[WEBS_CLASS_GRID3FILE1, WEBS_CLASS_GRID3FILE2];
const WEBS_CLASS_CTEXT_HYPERLINK: &str = "ctext-hyperlink";

/// Submit the search opportunities form to the WEBS portal.
pub(crate) async fn submit_search_opps(client: &Client, response: HttpResponse) -> Result<HttpResponse, BoxError> {
    let url = response.url().clone();
    debug!("WEBS search opps form URL: {url}");

    let text = match response.text() {
        Ok(t) => t,
        Err(e) => {
            error!("Failed to read WEBS search opps form: {e}");
            return Err(e.into());
        }
    };

    let mut form = match Form::from_unparsed_form_name(&url, text, "Form1") {
        Ok(form) => form,
        Err(e) => {
            error!("Failed to parse WEBS search opps form: {e}");
            return Err(e);
        }
    };

    // Search all commodity codes and counties
    form.set(WEBS_RAD_COMM_CODES_PARAM, "1");
    form.set(WEBS_RAD_COUNTIES_PARAM, "1");

    let response = match client.request(form.method, form.url).form(&form.fields).send().await.error_for_status() {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to submit WEBS search opps form: {e}");
            return Err(e);
        }
    };

    Ok(response)
}

/// Parse an opportunity listing page and insert next requests for each opportunity detail page.
pub(crate) fn parse_opportunity_listing_page(
    document: &RcDom,
    page_url: &Url,
    crawl_parameters: &CrawlParameters,
    next_requests: &mut Vec<NextRequest>,
) -> Result<(), BoxError> {
    // Each opportunity is in a <tr> with class name Grid3File1 or Grid3File2.
    for opp_class in WEBS_OPPORTUNITY_CLASSES {
        for opp_tr in document.tag("tr").class(*opp_class).find_all() {
            let Some(a) = opp_tr.tag("a").class(WEBS_CLASS_CTEXT_HYPERLINK).find() else {
                warn!("No hyperlink found for opportunity: {opp_tr:?}");
                continue;
            };

            let Some(href) = a.get("href") else {
                warn!("Opportunity link missing href attribute: {a:?}");
                continue;
            };

            let Ok(opp_url) = page_url.join(&href) else {
                warn!("Failed to parse opportunity URL: {href}");
                continue;
            };

            next_requests.push(NextRequest {
                operation: Operation::Webs(WebsOperation::FetchOpportunityDetailPage),
                url: Some(opp_url.to_string()),
                crawl: crawl_parameters.clone(),
            })
        }
    }

    Ok(())
}

pub(crate) fn find_opportunity_next_pages(document: &RcDom) -> Result<Vec<FormEvent>, BoxError> {
    let mut next_pages = vec![];

    // The pager links are within a <tr> with class Grid3Pager, and are <a> elements with an href
    // similar to "javascript:__doPostBack(&#39;DataGrid1$_ctl104$_ctl2&#39;,&#39;&#39;)"
    for tr in document.tag("tr").class(WEBS_CLASS_GRID3PAGER).find_all() {
        for a in tr.tag("a").find_all() {
            let Some(href) = a.get("href") else {
                warn!("No href attribute found for pager link: {a:?}");
                continue;
            };

            let post_back_args = match href.strip_prefix("javascript:__doPostBack(").and_then(|s| s.strip_suffix(')')) {
                Some(s) => s,
                None => {
                    warn!("Unexpected pager link href: {href}");
                    continue;
                }
            };

            // If this is still URL encoded, decode it.
            let post_back_args = post_back_args.replace("&#39;", "'");

            let parts: Vec<&str> = post_back_args.split(',').collect();
            if parts.len() != 2 {
                warn!("Unexpected pager link format: {post_back_args}");
                continue;
            }

            let event_target = parts[0].strip_prefix('\'').and_then(|s| s.strip_suffix('\''));
            let event_argument = parts[1].strip_prefix('\'').and_then(|s| s.strip_suffix('\''));

            let Some(event_target) = event_target else {
                warn!("Unexpected pager link format: {post_back_args}");
                continue;
            };

            let Some(event_argument) = event_argument else {
                warn!("Unexpected pager link format: {post_back_args}");
                continue;
            };
            next_pages.push(FormEvent {
                target: event_target.to_string(),
                argument: event_argument.to_string(),
            });
        }
    }

    Ok(next_pages)
}

#[cfg(test)]
mod tests {
    use {
        super::{find_opportunity_next_pages, parse_opportunity_listing_page},
        crate::{
            httpext::CookieStore,
            shapes::{default_user_agent, CrawlParameters},
            soup::parse_html_str,
        },
        reqwest::Url,
    };

    #[test_log::test]
    fn page1() {
        const PAGE1: &str = include_str!("webs-search-bids-page1.html");
        let document = parse_html_str(PAGE1);
        let mut next_requests = vec![];
        let url = Url::parse("https://pr-webs-vendor.des.wa.gov/Search_Bid.aspx").unwrap();
        let crawl_parameters = CrawlParameters {
            crawl_id: Some("test".to_string()),
            user_agent: default_user_agent(),
            cookies: CookieStore::default(),
        };

        parse_opportunity_listing_page(&document, &url, &crawl_parameters, &mut next_requests).unwrap();
        assert_eq!(next_requests.len(), 100);

        let form_events = find_opportunity_next_pages(&document).unwrap();
        assert_eq!(form_events.len(), 2);

        assert_eq!(form_events[0].target, "DataGrid1$_ctl104$_ctl1");
        assert_eq!(form_events[0].argument, "");
        assert_eq!(form_events[1].target, "DataGrid1$_ctl104$_ctl2");
        assert_eq!(form_events[1].argument, "");
    }
}
