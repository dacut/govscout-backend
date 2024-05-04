//! WEBS login page handling.
use {
    crate::{
        httpext::{Client, Form, LogConfig, Response as HttpResponse, ResponseExt},
        BoxError,
        webs::FORM_NAME_FORM1,
    },
    log::*,
};

const SSM_WEBS_USERNAME_PARAM: &str = "Webs/Username";
const SSM_WEBS_PASSWORD_PARAM: &str = "Webs/Password";
const WEBS_TXT_EMAIL_PARAM: &str = "txtEmail";
const WEBS_TXT_PASSWORD_PARAM: &str = "txtPassword";

/// Submit the login form to the WEBS portal.
pub(crate) async fn submit_login(
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

    let mut form = match Form::from_unparsed_form_name(&url, text, FORM_NAME_FORM1) {
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

    let response = match client.request(form.method, form.url).form(&form.fields).send().await.error_for_status() {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to submit WEBS login form: {e}");
            return Err(e);
        }
    };

    Ok(response)
}
