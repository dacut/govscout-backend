//! WEBS home page handling.
use {
    crate::{
        soup::{parse_html_str, NodeExt, QueryBuilderExt},
        webs::SEARCH_BID_PATH,
        BoxError,
    },
    log::*,
    reqwest::Url,
};

pub(crate) fn find_search_url(base_url: &Url, text: &str) -> Result<Url, BoxError> {
    let document = parse_html_str(text);

    // The link to the opportunity overview page is in (as of this writing):
    // html > body > form#Form1 > table > tr[1] > td.leftnav-bg-light > table > tr[10] > td.leftnav-bg > a#leftnav_hypSearch.leftnav-hyperlink
    // We just look for that last <a> tag.
    let Some(search_url) = document.tag("a").attr("id", "leftnav_hypSearch").class("leftnav-hyperlink").find() else {
        warn!(
            r#"Opportunity overview tag (<a id="leftnav_hypSearch" class="leftnav-hyperlink">) not found; using default search URL"#
        );
        return get_default_search_url(base_url);
    };

    let Some(search_url) = search_url.get("href") else {
        warn!(
            r#"Opportunity overview tag (<a id="leftnav_hypSearch" class="leftnav-hyperlink">) has no href attribute; using default search URL"#
        );
        return get_default_search_url(base_url);
    };

    let Ok(search_url) = Url::parse(&search_url) else {
        // URL is relative.
        return Ok(base_url.join(&search_url)?);
    };

    Ok(search_url)
}

fn get_default_search_url(base_url: &Url) -> Result<Url, BoxError> {
    // Construct the URL from the base URL plus the expected path.
    Ok(base_url.join(SEARCH_BID_PATH)?)
}

#[cfg(test)]
mod tests {
    use {super::find_search_url, reqwest::Url};

    #[test_log::test]
    fn test_find_search_url() {
        const PAGE: &str = include_str!("webs-home.html");
        let base_url = Url::parse("https://www.example.com/").unwrap();
        let search_url = find_search_url(&base_url, PAGE).unwrap();

        assert_eq!(search_url.to_string().as_str(), "https://www.example.com/Search_Bid.aspx");
    }
}
