use {
    crate::{
        httpext::{CookieStoreRwLock, LogConfig, RequestBuilder, Response},
        BoxError,
    },
    reqwest::{
        dns::Resolve,
        header::{HeaderMap, HeaderValue},
        redirect::Policy as RedirectPolicy,
        Certificate, Error as ReqwestError, Identity, IntoUrl, Method, Proxy, Request,
    },
    std::{
        clone::Clone,
        net::{IpAddr, SocketAddr},
        sync::Arc,
        time::Duration,
    },
};

/// Track a Reqwest [ClientBuilder][reqwest::ClientBuilder] along with a cookie store.
#[derive(Debug)]
pub struct ClientBuilder {
    /// The Reqwest client builder.
    pub builder: reqwest::ClientBuilder,

    /// The cookie store.
    pub cookie_store: Arc<CookieStoreRwLock>,

    /// Log configuration.
    pub log_config: Option<LogConfig>,

    /// The crawl id of the current crawl.
    pub crawl_id: String,
}

/// Track a Reqwest [Client][reqwest::Client] along with a cookie store.
#[derive(Clone, Debug)]
pub struct Client {
    /// The Reqwest client.
    pub client: reqwest::Client,

    /// The cookie store.
    pub cookie_store: Arc<CookieStoreRwLock>,

    /// Log configuration.
    pub log_config: Option<LogConfig>,

    /// The crawl id of the current crawl.
    pub crawl_id: String,
}

impl ClientBuilder {
    /// Create a new `ClientBuilder` wrapping the given [`CookieStoreRwLock`] and crawl id.
    pub fn new<S: Into<String>>(cookie_store: Arc<CookieStoreRwLock>, crawl_id: S) -> Self {
        let builder = reqwest::ClientBuilder::new().cookie_provider(cookie_store.clone());

        Self {
            builder,
            cookie_store,
            log_config: None,
            crawl_id: crawl_id.into(),
        }
    }

    /// Returns a `Client` that uses this `ClientBuilder` configuration.
    ///
    /// # Errors
    ///
    /// This method fails if a TLS backend cannot be initialized, or the resolver
    /// cannot load the system configuration.
    pub fn build(self) -> Result<Client, ReqwestError> {
        let client = self.builder.build()?;
        Ok(Client {
            client,
            cookie_store: self.cookie_store,
            log_config: self.log_config,
            crawl_id: self.crawl_id,
        })
    }

    /// Sets the `User-Agent` header to be used by this client.
    ///
    /// # Example
    ///
    /// ```rust
    /// # async fn doc() -> Result<(), reqwest::Error> {
    /// // Name your user agent after your app?
    /// static APP_USER_AGENT: &str = concat!(
    ///     env!("CARGO_PKG_NAME"),
    ///     "/",
    ///     env!("CARGO_PKG_VERSION"),
    /// );
    ///
    /// let client = reqwest::Client::builder()
    ///     .user_agent(APP_USER_AGENT)
    ///     .build()?;
    /// let res = client.get("https://www.rust-lang.org").send().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn user_agent<V>(mut self, value: V) -> ClientBuilder
    where
        V: TryInto<HeaderValue>,
        V::Error: Into<http::Error>,
    {
        self.builder = self.builder.user_agent(value);
        self
    }

    /// Sets the default headers for every request.
    ///
    /// # Example
    ///
    /// ```rust
    /// use reqwest::header;
    /// # async fn doc() -> Result<(), reqwest::Error> {
    /// let mut headers = header::HeaderMap::new();
    /// headers.insert("X-MY-HEADER", header::HeaderValue::from_static("value"));
    ///
    /// // Consider marking security-sensitive headers with `set_sensitive`.
    /// let mut auth_value = header::HeaderValue::from_static("secret");
    /// auth_value.set_sensitive(true);
    /// headers.insert(header::AUTHORIZATION, auth_value);
    ///
    /// // get a client builder
    /// let client = reqwest::Client::builder()
    ///     .default_headers(headers)
    ///     .build()?;
    /// let res = client.get("https://www.rust-lang.org").send().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn default_headers(mut self, headers: HeaderMap) -> ClientBuilder {
        self.builder = self.builder.default_headers(headers);
        self
    }

    /// Enable auto gzip decompression by checking the `Content-Encoding` response header.
    ///
    /// If auto gzip decompression is turned on:
    ///
    /// - When sending a request and if the request's headers do not already contain
    ///   an `Accept-Encoding` **and** `Range` values, the `Accept-Encoding` header is set to `gzip`.
    ///   The request body is **not** automatically compressed.
    /// - When receiving a response, if its headers contain a `Content-Encoding` value of
    ///   `gzip`, both `Content-Encoding` and `Content-Length` are removed from the
    ///   headers' set. The response body is automatically decompressed.
    #[inline(always)]
    pub fn gzip(mut self, enable: bool) -> ClientBuilder {
        self.builder = self.builder.gzip(enable);
        self
    }

    /// Enable auto brotli decompression by checking the `Content-Encoding` response header.
    ///
    /// If auto brotli decompression is turned on:
    ///
    /// - When sending a request and if the request's headers do not already contain
    ///   an `Accept-Encoding` **and** `Range` values, the `Accept-Encoding` header is set to `br`.
    ///   The request body is **not** automatically compressed.
    /// - When receiving a response, if its headers contain a `Content-Encoding` value of
    ///   `br`, both `Content-Encoding` and `Content-Length` are removed from the
    ///   headers' set. The response body is automatically decompressed.
    #[inline(always)]
    pub fn brotli(mut self, enable: bool) -> ClientBuilder {
        self.builder = self.builder.brotli(enable);
        self
    }

    /// Enable auto deflate decompression by checking the `Content-Encoding` response header.
    ///
    /// If auto deflate decompression is turned on:
    ///
    /// - When sending a request and if the request's headers do not already contain
    ///   an `Accept-Encoding` **and** `Range` values, the `Accept-Encoding` header is set to `deflate`.
    ///   The request body is **not** automatically compressed.
    /// - When receiving a response, if it's headers contain a `Content-Encoding` value that
    ///   equals to `deflate`, both values `Content-Encoding` and `Content-Length` are removed from the
    ///   headers' set. The response body is automatically decompressed.
    #[inline(always)]
    pub fn deflate(mut self, enable: bool) -> ClientBuilder {
        self.builder = self.builder.deflate(enable);
        self
    }

    /// Disable auto response body gzip decompression.
    #[inline(always)]
    pub fn no_gzip(mut self) -> ClientBuilder {
        self.builder = self.builder.no_gzip();
        self
    }

    /// Disable auto response body brotli decompression.
    #[inline(always)]
    pub fn no_brotli(mut self) -> ClientBuilder {
        self.builder = self.builder.no_brotli();
        self
    }

    /// Disable auto response body deflate decompression.
    #[inline(always)]
    pub fn no_deflate(mut self) -> ClientBuilder {
        self.builder = self.builder.no_deflate();
        self
    }

    /// Set a `RedirectPolicy` for this client.
    ///
    /// Default will follow redirects up to a maximum of 10.
    #[inline(always)]
    pub fn redirect(mut self, policy: RedirectPolicy) -> ClientBuilder {
        self.builder = self.builder.redirect(policy);
        self
    }

    /// Enable or disable automatic setting of the `Referer` header.
    ///
    /// Default is `true`.
    #[inline(always)]
    pub fn referer(mut self, enable: bool) -> ClientBuilder {
        self.builder = self.builder.referer(enable);
        self
    }

    /// Add a `Proxy` to the list of proxies the `Client` will use.
    ///
    /// # Note
    ///
    /// Adding a proxy will disable the automatic usage of the "system" proxy.
    #[inline(always)]
    pub fn proxy(mut self, proxy: Proxy) -> ClientBuilder {
        self.builder = self.builder.proxy(proxy);
        self
    }

    /// Clear all `Proxies`, so `Client` will use no proxy anymore.
    ///
    /// # Note
    /// To add a proxy exclusion list, use [`Proxy::no_proxy()`]
    /// on all desired proxies instead.
    ///
    /// This also disables the automatic usage of the "system" proxy.
    #[inline(always)]
    pub fn no_proxy(mut self) -> ClientBuilder {
        self.builder = self.builder.no_proxy();
        self
    }

    /// Enables a request timeout.
    ///
    /// The timeout is applied from when the request starts connecting until the
    /// response body has finished.
    ///
    /// Default is no timeout.
    #[inline(always)]
    pub fn timeout(mut self, timeout: Duration) -> ClientBuilder {
        self.builder = self.builder.timeout(timeout);
        self
    }

    /// Set a timeout for only the connect phase of a `Client`.
    ///
    /// Default is `None`.
    ///
    /// # Note
    ///
    /// This **requires** the futures be executed in a tokio runtime with
    /// a tokio timer enabled.
    #[inline(always)]
    pub fn connect_timeout(mut self, timeout: Duration) -> ClientBuilder {
        self.builder = self.builder.connect_timeout(timeout);
        self
    }

    /// Set whether connections should emit verbose logs.
    ///
    /// Enabling this option will emit [log][] messages at the `TRACE` level
    /// for read and write operations on connections.
    ///
    /// [log]: https://crates.io/crates/log
    #[inline(always)]
    pub fn connection_verbose(mut self, verbose: bool) -> ClientBuilder {
        self.builder = self.builder.connection_verbose(verbose);
        self
    }

    /// Set an optional timeout for idle sockets being kept-alive.
    ///
    /// Pass `None` to disable timeout.
    ///
    /// Default is 90 seconds.
    #[inline(always)]
    pub fn pool_idle_timeout<D>(mut self, val: D) -> ClientBuilder
    where
        D: Into<Option<Duration>>,
    {
        self.builder = self.builder.pool_idle_timeout(val);
        self
    }

    /// Sets the maximum idle connection per host allowed in the pool.
    #[inline(always)]
    pub fn pool_max_idle_per_host(mut self, max: usize) -> ClientBuilder {
        self.builder = self.builder.pool_max_idle_per_host(max);
        self
    }

    /// Send headers as title case instead of lowercase.
    #[inline(always)]
    pub fn http1_title_case_headers(mut self) -> ClientBuilder {
        self.builder = self.builder.http1_title_case_headers();
        self
    }

    /// Set whether HTTP/1 connections will accept obsolete line folding for
    /// header values.
    ///
    /// Newline codepoints (`\r` and `\n`) will be transformed to spaces when
    /// parsing.
    #[inline(always)]
    pub fn http1_allow_obsolete_multiline_headers_in_responses(mut self, value: bool) -> ClientBuilder {
        self.builder = self.builder.http1_allow_obsolete_multiline_headers_in_responses(value);
        self
    }

    /// Sets whether invalid header lines should be silently ignored in HTTP/1 responses.
    #[inline(always)]
    pub fn http1_ignore_invalid_headers_in_responses(mut self, value: bool) -> ClientBuilder {
        self.builder = self.builder.http1_ignore_invalid_headers_in_responses(value);
        self
    }

    /// Set whether HTTP/1 connections will accept spaces between header
    /// names and the colon that follow them in responses.
    ///
    /// Newline codepoints (`\r` and `\n`) will be transformed to spaces when
    /// parsing.
    #[inline(always)]
    pub fn http1_allow_spaces_after_header_name_in_responses(mut self, value: bool) -> ClientBuilder {
        self.builder = self.builder.http1_allow_spaces_after_header_name_in_responses(value);
        self
    }

    /// Only use HTTP/1.
    #[inline(always)]
    pub fn http1_only(mut self) -> ClientBuilder {
        self.builder = self.builder.http1_only();
        self
    }

    /// Allow HTTP/0.9 responses
    #[inline(always)]
    pub fn http09_responses(mut self) -> ClientBuilder {
        self.builder = self.builder.http09_responses();
        self
    }

    /// Only use HTTP/2.
    #[inline(always)]
    pub fn http2_prior_knowledge(mut self) -> ClientBuilder {
        self.builder = self.builder.http2_prior_knowledge();
        self
    }

    /// Sets the `SETTINGS_INITIAL_WINDOW_SIZE` option for HTTP2 stream-level flow control.
    ///
    /// Default is currently 65,535 but may change internally to optimize for common uses.
    #[inline(always)]
    pub fn http2_initial_stream_window_size(mut self, sz: impl Into<Option<u32>>) -> ClientBuilder {
        self.builder = self.builder.http2_initial_stream_window_size(sz);
        self
    }

    /// Sets the max connection-level flow control for HTTP2
    ///
    /// Default is currently 65,535 but may change internally to optimize for common uses.
    #[inline(always)]
    pub fn http2_initial_connection_window_size(mut self, sz: impl Into<Option<u32>>) -> ClientBuilder {
        self.builder = self.builder.http2_initial_connection_window_size(sz);
        self
    }

    /// Sets whether to use an adaptive flow control.
    ///
    /// Enabling this will override the limits set in `http2_initial_stream_window_size` and
    /// `http2_initial_connection_window_size`.
    #[inline(always)]
    pub fn http2_adaptive_window(mut self, enabled: bool) -> ClientBuilder {
        self.builder = self.builder.http2_adaptive_window(enabled);
        self
    }

    /// Sets the maximum frame size to use for HTTP2.
    ///
    /// Default is currently 16,384 but may change internally to optimize for common uses.
    #[inline(always)]
    pub fn http2_max_frame_size(mut self, sz: impl Into<Option<u32>>) -> ClientBuilder {
        self.builder = self.builder.http2_max_frame_size(sz);
        self
    }

    /// Sets an interval for HTTP2 Ping frames should be sent to keep a connection alive.
    ///
    /// Pass `None` to disable HTTP2 keep-alive.
    /// Default is currently disabled.
    #[inline(always)]
    pub fn http2_keep_alive_interval(mut self, interval: impl Into<Option<Duration>>) -> ClientBuilder {
        self.builder = self.builder.http2_keep_alive_interval(interval);
        self
    }

    /// Sets a timeout for receiving an acknowledgement of the keep-alive ping.
    ///
    /// If the ping is not acknowledged within the timeout, the connection will be closed.
    /// Does nothing if `http2_keep_alive_interval` is disabled.
    /// Default is currently disabled.
    #[inline(always)]
    pub fn http2_keep_alive_timeout(mut self, timeout: Duration) -> ClientBuilder {
        self.builder = self.builder.http2_keep_alive_timeout(timeout);
        self
    }

    /// Sets whether HTTP2 keep-alive should apply while the connection is idle.
    ///
    /// If disabled, keep-alive pings are only sent while there are open request/responses streams.
    /// If enabled, pings are also sent when no streams are active.
    /// Does nothing if `http2_keep_alive_interval` is disabled.
    /// Default is `false`.
    #[inline(always)]
    pub fn http2_keep_alive_while_idle(mut self, enabled: bool) -> ClientBuilder {
        self.builder = self.builder.http2_keep_alive_while_idle(enabled);
        self
    }

    /// Set whether sockets have `TCP_NODELAY` enabled.
    ///
    /// Default is `true`.
    #[inline(always)]
    pub fn tcp_nodelay(mut self, enabled: bool) -> ClientBuilder {
        self.builder = self.builder.tcp_nodelay(enabled);
        self
    }

    /// Bind to a local IP Address.
    ///
    /// # Example
    ///
    /// ```
    /// use std::net::IpAddr;
    /// let local_addr = IpAddr::from([12, 4, 1, 8]);
    /// let client = reqwest::Client::builder()
    ///     .local_address(local_addr)
    ///     .build().unwrap();
    /// ```
    #[inline(always)]
    pub fn local_address<T>(mut self, addr: T) -> ClientBuilder
    where
        T: Into<Option<IpAddr>>,
    {
        self.builder = self.builder.local_address(addr);
        self
    }

    /// Bind to an interface by `SO_BINDTODEVICE`.
    ///
    /// # Example
    ///
    /// ```
    /// let interface = "lo";
    /// let client = reqwest::Client::builder()
    ///     .interface(interface)
    ///     .build().unwrap();
    /// ```
    #[cfg(any(target_os = "android", target_os = "fuchsia", target_os = "linux"))]
    #[inline(always)]
    pub fn interface(mut self, interface: &str) -> ClientBuilder {
        self.builder = self.builder.interface(interface);
        self
    }

    /// Set that all sockets have `SO_KEEPALIVE` set with the supplied duration.
    ///
    /// If `None`, the option will not be set.
    #[inline(always)]
    pub fn tcp_keepalive<D>(mut self, val: D) -> ClientBuilder
    where
        D: Into<Option<Duration>>,
    {
        self.builder = self.builder.tcp_keepalive(val);
        self
    }

    /// Add a custom root certificate.
    ///
    /// This can be used to connect to a server that has a self-signed
    /// certificate for example.
    #[inline(always)]
    pub fn add_root_certificate(mut self, cert: Certificate) -> ClientBuilder {
        self.builder = self.builder.add_root_certificate(cert);
        self
    }

    /// Controls the use of built-in/preloaded certificates during certificate validation.
    ///
    /// Defaults to `true` -- built-in system certs will be used.
    ///
    /// # Bulk Option
    ///
    /// If this value is `true`, _all_ enabled system certs configured with Cargo
    /// features will be loaded.
    ///
    /// You can set this to `false`, and enable only a specific source with
    /// individual methods. Do that will prevent other sources from being loaded
    /// even if their feature Cargo feature is enabled.
    #[inline(always)]
    pub fn tls_built_in_root_certs(mut self, tls_built_in_root_certs: bool) -> ClientBuilder {
        self.builder = self.builder.tls_built_in_root_certs(tls_built_in_root_certs);
        self
    }

    /// Sets whether to load webpki root certs with rustls.
    ///
    /// If the feature is enabled, this value is `true` by default.
    #[cfg(feature = "rustls-tls-webpki-roots")]
    #[cfg_attr(docsrs, doc(cfg(feature = "rustls-tls-webpki-roots")))]
    #[inline(always)]
    pub fn tls_built_in_webpki_certs(mut self, enabled: bool) -> ClientBuilder {
        self.builder = self.builder.tls_built_in_webpki_certs(enabled);
        self
    }

    /// Sets whether to load native root certs with rustls.
    ///
    /// If the feature is enabled, this value is `true` by default.
    #[cfg(feature = "rustls-tls-native-roots")]
    #[cfg_attr(docsrs, doc(cfg(feature = "rustls-tls-native-roots")))]
    #[inline(always)]
    pub fn tls_built_in_native_certs(mut self, enabled: bool) -> ClientBuilder {
        self.builder = self.builder.tls_built_in_native_certs(enabled);
        self
    }

    /// Sets the identity to be used for client certificate authentication.
    #[inline(always)]
    pub fn identity(mut self, identity: Identity) -> ClientBuilder {
        self.builder = self.builder.identity(identity);
        self
    }

    /// Controls the use of TLS server name indication.
    ///
    /// Defaults to `true`.
    #[inline(always)]
    pub fn tls_sni(mut self, tls_sni: bool) -> ClientBuilder {
        self.builder = self.builder.tls_sni(tls_sni);
        self
    }

    /// Set the minimum required TLS version for connections.
    ///
    /// By default the TLS backend's own default is used.
    ///
    /// # Errors
    ///
    /// A value of `tls::Version::TLS_1_3` will cause an error with the
    /// `native-tls`/`default-tls` backend. This does not mean the version
    /// isn't supported, just that it can't be set as a minimum due to
    /// technical limitations.
    #[inline(always)]
    pub fn min_tls_version(mut self, version: reqwest::tls::Version) -> ClientBuilder {
        self.builder = self.builder.min_tls_version(version);
        self
    }

    /// Set the maximum allowed TLS version for connections.
    ///
    /// By default there's no maximum.
    ///
    /// # Errors
    ///
    /// A value of `tls::Version::TLS_1_3` will cause an error with the
    /// `native-tls`/`default-tls` backend. This does not mean the version
    /// isn't supported, just that it can't be set as a maximum due to
    /// technical limitations.
    ///
    /// Cannot set a maximum outside the protocol versions supported by
    /// `rustls` with the `rustls-tls` backend.
    #[inline(always)]
    pub fn max_tls_version(mut self, version: reqwest::tls::Version) -> ClientBuilder {
        self.builder = self.builder.max_tls_version(version);
        self
    }

    /// Add TLS information as `TlsInfo` extension to responses.
    #[inline(always)]
    pub fn tls_info(mut self, tls_info: bool) -> ClientBuilder {
        self.builder = self.builder.tls_info(tls_info);
        self
    }

    /// Restrict the Client to be used with HTTPS only requests.
    ///
    /// Defaults to false.
    #[inline(always)]
    pub fn https_only(mut self, enabled: bool) -> ClientBuilder {
        self.builder = self.builder.https_only(enabled);
        self
    }

    /// Override DNS resolution for specific domains to a particular IP address.
    ///
    /// Warning
    ///
    /// Since the DNS protocol has no notion of ports, if you wish to send
    /// traffic to a particular port you must include this port in the URL
    /// itself, any port in the overridden addr will be ignored and traffic sent
    /// to the conventional port for the given scheme (e.g. 80 for http).
    #[inline(always)]
    pub fn resolve(mut self, domain: &str, addr: SocketAddr) -> ClientBuilder {
        self.builder = self.builder.resolve(domain, addr);
        self
    }

    /// Enables the [hickory-dns](hickory_resolver) async resolver instead of a default threadpool
    /// using `getaddrinfo`.
    ///
    /// If the `hickory-dns` feature is turned on, the default option is enabled.
    ///
    /// # Optional
    ///
    /// This requires the optional `hickory-dns` feature to be enabled
    #[cfg(feature = "hickory-dns")]
    #[cfg_attr(docsrs, doc(cfg(feature = "hickory-dns")))]
    pub fn hickory_dns(mut self, enable: bool) -> ClientBuilder {
        self.builder = self.builder.hickory_dns(enable);
        self
    }

    /// Disables the hickory-dns async resolver.
    ///
    /// This method exists even if the optional `hickory-dns` feature is not enabled.
    /// This can be used to ensure a `Client` doesn't use the hickory-dns async resolver
    /// even if another dependency were to enable the optional `hickory-dns` feature.
    #[inline(always)]
    pub fn no_hickory_dns(mut self) -> ClientBuilder {
        self.builder = self.builder.no_hickory_dns();
        self
    }

    /// Override DNS resolution for specific domains to particular IP addresses.
    ///
    /// Warning
    ///
    /// Since the DNS protocol has no notion of ports, if you wish to send
    /// traffic to a particular port you must include this port in the URL
    /// itself, any port in the overridden addresses will be ignored and traffic sent
    /// to the conventional port for the given scheme (e.g. 80 for http).
    #[inline(always)]
    pub fn resolve_to_addrs(mut self, domain: &str, addrs: &[SocketAddr]) -> ClientBuilder {
        self.builder = self.builder.resolve_to_addrs(domain, addrs);
        self
    }

    /// Override the DNS resolver implementation.
    ///
    /// Pass an `Arc` wrapping a trait object implementing `Resolve`.
    /// Overrides for specific names passed to `resolve` and `resolve_to_addrs` will
    /// still be applied on top of this resolver.
    #[inline(always)]
    pub fn dns_resolver<R: Resolve + 'static>(mut self, resolver: Arc<R>) -> ClientBuilder {
        self.builder = self.builder.dns_resolver(resolver);
        self
    }
}

impl Client {
    /// Convenience method to make a `GET` request to a URL.
    ///
    /// # Errors
    ///
    /// This method fails whenever the supplied `Url` cannot be parsed.
    #[inline(always)]
    pub fn get<U: IntoUrl>(&self, url: U) -> RequestBuilder {
        RequestBuilder {
            builder: self.client.get(url),
            cookie_store: self.cookie_store.clone(),
            log_config: self.log_config.clone(),
            crawl_id: self.crawl_id.clone(),
        }
    }

    /// Convenience method to make a `POST` request to a URL.
    ///
    /// # Errors
    ///
    /// This method fails whenever the supplied `Url` cannot be parsed.
    #[inline(always)]
    pub fn post<U: IntoUrl>(&self, url: U) -> RequestBuilder {
        RequestBuilder {
            builder: self.client.post(url),
            cookie_store: self.cookie_store.clone(),
            log_config: self.log_config.clone(),
            crawl_id: self.crawl_id.clone(),
        }
    }

    /// Convenience method to make a `PUT` request to a URL.
    ///
    /// # Errors
    ///
    /// This method fails whenever the supplied `Url` cannot be parsed.
    #[inline(always)]
    pub fn put<U: IntoUrl>(&self, url: U) -> RequestBuilder {
        RequestBuilder {
            builder: self.client.put(url),
            cookie_store: self.cookie_store.clone(),
            log_config: self.log_config.clone(),
            crawl_id: self.crawl_id.clone(),
        }
    }

    /// Convenience method to make a `PATCH` request to a URL.
    ///
    /// # Errors
    ///
    /// This method fails whenever the supplied `Url` cannot be parsed.
    #[inline(always)]
    pub fn patch<U: IntoUrl>(&self, url: U) -> RequestBuilder {
        RequestBuilder {
            builder: self.client.patch(url),
            cookie_store: self.cookie_store.clone(),
            log_config: self.log_config.clone(),
            crawl_id: self.crawl_id.clone(),
        }
    }

    /// Convenience method to make a `DELETE` request to a URL.
    ///
    /// # Errors
    ///
    /// This method fails whenever the supplied `Url` cannot be parsed.
    #[inline(always)]
    pub fn delete<U: IntoUrl>(&self, url: U) -> RequestBuilder {
        RequestBuilder {
            builder: self.client.delete(url),
            cookie_store: self.cookie_store.clone(),
            log_config: self.log_config.clone(),
            crawl_id: self.crawl_id.clone(),
        }
    }

    /// Convenience method to make a `HEAD` request to a URL.
    ///
    /// # Errors
    ///
    /// This method fails whenever the supplied `Url` cannot be parsed.
    #[inline(always)]
    pub fn head<U: IntoUrl>(&self, url: U) -> RequestBuilder {
        RequestBuilder {
            builder: self.client.head(url),
            cookie_store: self.cookie_store.clone(),
            log_config: self.log_config.clone(),
            crawl_id: self.crawl_id.clone(),
        }
    }

    /// Start building a `Request` with the `Method` and `Url`.
    ///
    /// Returns a `RequestBuilder`, which will allow setting headers and
    /// the request body before sending.
    ///
    /// # Errors
    ///
    /// This method fails whenever the supplied `Url` cannot be parsed.
    #[inline(always)]
    pub fn request<U: IntoUrl>(&self, method: Method, url: U) -> RequestBuilder {
        RequestBuilder {
            builder: self.client.request(method, url),
            cookie_store: self.cookie_store.clone(),
            log_config: self.log_config.clone(),
            crawl_id: self.crawl_id.clone(),
        }
    }

    /// Executes a `Request`.
    ///
    /// A `Request` can be built manually with `Request::new()` or obtained
    /// from a RequestBuilder with `RequestBuilder::build()`.
    ///
    /// You should prefer to use the `RequestBuilder` and
    /// `RequestBuilder::send()`.
    ///
    /// # Errors
    ///
    /// This method fails if there was an error while sending request,
    /// redirect loop was detected or redirect limit was exhausted.
    #[inline(always)]
    pub async fn execute(&self, request: Request) -> Result<Response, BoxError> {
        let method = request.method().clone();
        let url = request.url().clone();
        Response::new(self.client.execute(request).await?, self.crawl_id.clone(), method, url, self.log_config.clone())
            .await
    }
}

#[cfg(test)]
mod tests {
    use {
        super::ClientBuilder,
        crate::httpext::{CookieStore, CookieStoreRwLock},
        httpmock::prelude::*,
        log::debug,
        serde::Serialize,
        std::sync::Arc,
    };

    #[derive(Serialize)]
    struct CookieStoreTest {
        cookies: CookieStore,
    }

    #[tokio::test]
    #[test_log::test]
    async fn cookie_persistence() {
        let server = MockServer::start();
        let cookie_mock = server.mock(|when, then| {
            when.method(GET).path("/");
            then.status(200)
                .header("content-type", "text/html")
                .header("set-cookie", "TestCookie=Value; Domain=127.0.0.1; Expires=Sat, 31 Dec 2050 httponly")
                .body("<html></html>");
        });

        let cookie_store = CookieStore::default();
        let cookie_store: Arc<CookieStoreRwLock> = Arc::new(cookie_store.into());

        let client =
            ClientBuilder::new(cookie_store.clone(), "test".to_string()).build().expect("Failed to build client");

        debug!("Using URL: {}", server.url("/"));
        let response = client.get(server.url("/")).send().await.unwrap();
        assert_eq!(response.status(), 200);
        cookie_mock.assert();

        drop(client);

        let cookie_store = cookie_store.read().unwrap();

        let cst = CookieStoreTest {
            cookies: cookie_store.clone(),
        };
        let ser_data = serde_json::to_string(&cst).unwrap();
        assert_eq!(
            ser_data.as_str(),
            r#"{"cookies":[{"raw_cookie":"TestCookie=Value; Domain=127.0.0.1","path":["/",false],"domain":{"Suffix":"127.0.0.1"},"expires":"SessionEnd"}]}"#
        );
    }
}
