use {
    crate::{
        httpext::{Client, CookieStoreRwLock, LogConfig, Response},
        BoxError,
    },
    reqwest::{
        header::{HeaderMap, HeaderName, HeaderValue},
        Body, Error as ReqwestError, Request, Version,
    },
    serde::Serialize,
    std::{fmt::Display, sync::Arc, time::Duration},
};

/// A builder to construct the properties of a `Request`.
///
/// To construct a `RequestBuilder`, refer to the `Client` documentation.
#[derive(Debug)]
#[must_use = "RequestBuilder does nothing until you 'send' it"]
pub struct RequestBuilder {
    /// The Reqwest request builder.
    pub builder: reqwest::RequestBuilder,

    /// The cookie store.
    pub cookie_store: Arc<CookieStoreRwLock>,

    /// The log configuration to use.
    pub log_config: Option<LogConfig>,

    /// The crawl id of the current crawl.
    pub crawl_id: String,
}

impl RequestBuilder {
    /// Add a `Header` to this Request.
    #[inline(always)]
    pub fn header<K, V>(mut self, key: K, value: V) -> RequestBuilder
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: Into<http::Error>,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        self.builder = self.builder.header(key, value);
        self
    }

    /// Add a set of Headers to the existing ones on this Request.
    ///
    /// The headers will be merged in to any already set.
    #[inline(always)]
    pub fn headers(mut self, headers: HeaderMap) -> RequestBuilder {
        self.builder = self.builder.headers(headers);
        self
    }

    /// Enable HTTP basic authentication.
    ///
    /// ```rust
    /// # use reqwest::Error;
    ///
    /// # async fn run() -> Result<(), Error> {
    /// let client = reqwest::Client::new();
    /// let resp = client.delete("http://httpbin.org/delete")
    ///     .basic_auth("admin", Some("good password"))
    ///     .send()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn basic_auth<U, P>(mut self, username: U, password: Option<P>) -> RequestBuilder
    where
        U: Display,
        P: Display,
    {
        self.builder = self.builder.basic_auth(username, password);
        self
    }

    /// Enable HTTP bearer authentication.
    #[inline(always)]
    pub fn bearer_auth<T>(mut self, token: T) -> RequestBuilder
    where
        T: Display,
    {
        self.builder = self.builder.bearer_auth(token);
        self
    }

    /// Set the request body.
    #[inline(always)]
    pub fn body<T: Into<Body>>(mut self, body: T) -> RequestBuilder {
        self.builder = self.builder.body(body);
        self
    }

    /// Enables a request timeout.
    ///
    /// The timeout is applied from when the request starts connecting until the
    /// response body has finished. It affects only this request and overrides
    /// the timeout configured using `ClientBuilder::timeout()`.
    #[inline(always)]
    pub fn timeout(mut self, timeout: Duration) -> RequestBuilder {
        self.builder = self.builder.timeout(timeout);
        self
    }
    /// Sends a multipart/form-data body.
    ///
    /// ```
    /// # use reqwest::Error;
    ///
    /// # async fn run() -> Result<(), Error> {
    /// let client = reqwest::Client::new();
    /// let form = reqwest::multipart::Form::new()
    ///     .text("key3", "value3")
    ///     .text("key4", "value4");
    ///
    ///
    /// let response = client.post("your url")
    ///     .multipart(form)
    ///     .send()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "multipart")]
    #[cfg_attr(docsrs, doc(cfg(feature = "multipart")))]
    #[inline(always)]
    pub fn multipart(mut self, mut multipart: reqwest::multipart::Form) -> RequestBuilder {
        self.builder = self.builder.multipart(multipart);
        self
    }

    /// Modify the query string of the URL.
    ///
    /// Modifies the URL of this request, adding the parameters provided.
    /// This method appends and does not overwrite. This means that it can
    /// be called multiple times and that existing query parameters are not
    /// overwritten if the same key is used. The key will simply show up
    /// twice in the query string.
    /// Calling `.query(&[("foo", "a"), ("foo", "b")])` gives `"foo=a&foo=b"`.
    ///
    /// # Note
    /// This method does not support serializing a single key-value
    /// pair. Instead of using `.query(("key", "val"))`, use a sequence, such
    /// as `.query(&[("key", "val")])`. It's also possible to serialize structs
    /// and maps into a key-value pair.
    ///
    /// # Errors
    /// This method will fail if the object you provide cannot be serialized
    /// into a query string.
    #[inline(always)]
    pub fn query<T: Serialize + ?Sized>(mut self, query: &T) -> RequestBuilder {
        self.builder = self.builder.query(query);
        self
    }

    /// Set HTTP version
    #[inline(always)]
    pub fn version(mut self, version: Version) -> RequestBuilder {
        self.builder = self.builder.version(version);
        self
    }

    /// Send a form body.
    ///
    /// Sets the body to the url encoded serialization of the passed value,
    /// and also sets the `Content-Type: application/x-www-form-urlencoded`
    /// header.
    ///
    /// ```rust
    /// # use reqwest::Error;
    /// # use std::collections::HashMap;
    /// #
    /// # async fn run() -> Result<(), Error> {
    /// let mut params = HashMap::new();
    /// params.insert("lang", "rust");
    ///
    /// let client = reqwest::Client::new();
    /// let res = client.post("http://httpbin.org")
    ///     .form(&params)
    ///     .send()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// This method fails if the passed value cannot be serialized into
    /// url encoded format
    pub fn form<T: Serialize + ?Sized>(mut self, form: &T) -> RequestBuilder {
        self.builder = self.builder.form(form);
        self
    }

    /// Send a JSON body.
    ///
    /// # Optional
    ///
    /// This requires the optional `json` feature enabled.
    ///
    /// # Errors
    ///
    /// Serialization can fail if `T`'s implementation of `Serialize` decides to
    /// fail, or if `T` contains a map with non-string keys.
    #[cfg(feature = "json")]
    #[cfg_attr(docsrs, doc(cfg(feature = "json")))]
    pub fn json<T: Serialize + ?Sized>(mut self, json: &T) -> RequestBuilder {
        self.builder = self.builder.json(json);
        self
    }

    /// Disable CORS on fetching the request.
    ///
    /// # WASM
    ///
    /// This option is only effective with WebAssembly target.
    ///
    /// The [request mode][mdn] will be set to 'no-cors'.
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/API/Request/mode
    pub fn fetch_mode_no_cors(mut self) -> RequestBuilder {
        self.builder = self.builder.fetch_mode_no_cors();
        self
    }

    /// Build a `Request`, which can be inspected, modified and executed with
    /// `Client::execute()`.
    pub fn build(self) -> Result<Request, ReqwestError> {
        self.builder.build()
    }

    /// Constructs the Request and sends it to the target URL, returning a
    /// future Response.
    ///
    /// # Errors
    ///
    /// This method fails if there was an error while sending request,
    /// redirect loop was detected or redirect limit was exhausted.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use reqwest::Error;
    /// #
    /// # async fn run() -> Result<(), Error> {
    /// let response = reqwest::Client::new()
    ///     .get("https://hyper.rs")
    ///     .send()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send(self) -> Result<Response, BoxError> {
        let (client, request) = self.builder.build_split();
        let request = request?;
        let client = Client {
            client,
            cookie_store: self.cookie_store,
            log_config: self.log_config,
            crawl_id: self.crawl_id.clone(),
        };

        client.execute(request).await
    }
}
