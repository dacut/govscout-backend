use {
    crate::{
        soup::{parse_html_str, NodeExt, QueryBuilderExt},
        BoxError,
    },
    markup5ever_rcdom::{Handle, RcDom},
    reqwest::{Method, Url},
    serde::{
        ser::{SerializeMap, Serializer},
        Serialize,
    },
    std::collections::HashMap,
};

/// Representation of an HTML form.
#[derive(Clone, Debug)]
pub struct Form {
    pub id: Option<String>,
    pub handle: Handle,
    pub method: Method,
    pub url: Url,
    pub fields: HashMap<String, String>,
}

impl Form {
    /// Create a new `Form` instance from the URL of the response, the unparsed document associated with it, and the
    /// name of the form.
    pub fn from_unparsed_form_name(base_url: &Url, document: &str, name: &str) -> Result<Self, BoxError> {
        let document = parse_html_str(document);
        Self::from_form_name(base_url, &document, name)
    }

    /// Create a new `Form` instance from a URL of the response, the parsed document associated with it, and the name
    /// of the form.
    ///
    /// The `name` parameter is the value of the `name` attribute on the form element, not its `id`.
    pub fn from_form_name(base_url: &Url, document: &RcDom, name: &str) -> Result<Self, BoxError> {
        let Some(form) = document.tag("form").attr("name", name).find() else {
            return Err(format!("Form with name '{}' not found", name).into());
        };

        Self::from_form_node(base_url, document, form)
    }

    /// Create a new `Form` instance from a base URL and the document associated with it and the form element itself.
    pub fn from_form_node(base_url: &Url, document: &RcDom, form: Handle) -> Result<Self, BoxError> {
        let method = form.get("method").unwrap_or_else(|| "POST".to_string()).to_uppercase();
        let method = Method::from_bytes(method.as_bytes())?;
        let url = form.get("action");

        let mut fields = HashMap::new();
        let id = form.get("id");

        // Find all input fields within this form.
        for field in form.tag("input").find_all() {
            Self::handle_input_field(&mut fields, &field, id.as_deref(), true);
        }

        // TODO: Handle <select> elements

        for field in form.tag("textarea").find_all() {
            Self::handle_textarea_field(&mut fields, &field, id.as_deref(), true);
        }

        // Find fields outside of the form itself.
        if id.is_some() {
            for field in document.tag("input").find_all() {
                Self::handle_input_field(&mut fields, &field, id.as_deref(), false);
            }

            for field in document.tag("textarea").find_all() {
                Self::handle_textarea_field(&mut fields, &field, id.as_deref(), false);
            }
        }

        let url = if let Some(url) = url {
            base_url.join(&url)?
        } else {
            base_url.clone()
        };

        Ok(Self {
            id,
            handle: form,
            method,
            url,
            fields,
        })
    }

    fn handle_input_field(
        fields: &mut HashMap<String, String>,
        field: &Handle,
        form_id: Option<&str>,
        is_form_field: bool,
    ) {
        let Some(name) = field.get("name") else {
            return;
        };

        if let Some(field_form_id) = field.get("form") {
            if form_id != Some(field_form_id.as_str()) {
                // Field belongs to a different form.
                return;
            }
        } else if !is_form_field {
            return;
        }

        let r#type = field.get("type").unwrap_or_else(|| "text".to_string());

        match r#type.as_str() {
            "image" => {
                if let Some(x) = field.get("width") {
                    fields.insert(format!("{name}.x"), x);
                }
                if let Some(y) = field.get("height") {
                    fields.insert(format!("{name}.y"), y);
                }
            }
            "reset" | "submit" => (),
            _ => {
                let value = field.get("value").unwrap_or_default();
                fields.insert(name, value);
            }
        }
    }

    fn handle_textarea_field(
        fields: &mut HashMap<String, String>,
        field: &Handle,
        form_id: Option<&str>,
        is_form_field: bool,
    ) {
        let Some(name) = field.get("name") else {
            return;
        };

        if let Some(field_form_id) = field.get("form") {
            if form_id != Some(field_form_id.as_str()) {
                // Field belongs to a different form.
                return;
            }
        } else if !is_form_field {
            return;
        }

        let value = field.text();
        fields.insert(name, value);
    }

    /// Set a parameter in this form.
    ///
    /// The return value indicates whether an existing parameter was replaced.
    pub fn set<S: Into<String>>(&mut self, name: &str, value: S) -> bool {
        self.fields.insert(name.to_string(), value.into()).is_some()
    }
}

impl Serialize for Form {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut m = serializer.serialize_map(Some(self.fields.len()))?;

        for (name, field) in &self.fields {
            m.serialize_entry(name, field)?;
        }

        m.end()
    }
}

#[cfg(test)]
mod tests {
    use {
        super::Form,
        reqwest::{Method, Url},
    };

    const WEBS_LOGIN_PAGE: &str = include_str!("webs-login-page.html");

    #[test]
    fn webs_parse() {
        let base_url = Url::parse("https://domain1.example.com/gateway.html").unwrap();
        let form = Form::from_unparsed_form_name(&base_url, WEBS_LOGIN_PAGE, "Form1").unwrap();
        assert_eq!(form.url.to_string().as_str(), "https://domain1.example.com/LoginPage.aspx");
        assert_eq!(form.method, Method::POST);

        assert_eq!(form.fields.get("txtEmail").unwrap().as_str(), "");
        assert_eq!(form.fields.get("txtPassword").unwrap().as_str(), "");
        assert_eq!(form.fields.get("__EVENTTARGET").unwrap().as_str(), "EventTarget");
        assert_eq!(form.fields.get("__EVENTARGUMENT").unwrap().as_str(), "EventArgument");
        assert_eq!(form.fields.get("__VIEWSTATE").unwrap().as_str(), "ViewState");
        assert_eq!(form.fields.get("__VIEWSTATEGENERATOR").unwrap().as_str(), "ViewStateGenerator");
        assert_eq!(form.fields.get("__EVENTVALIDATION").unwrap().as_str(), "EventValidation");
        assert_eq!(form.fields.get("Image1.x").unwrap().as_str(), "63");
        assert_eq!(form.fields.get("Image1.y").unwrap().as_str(), "15");
    }
}
