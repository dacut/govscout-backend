use {
    crate::{
        httpext::{log_aws_err, LogConfig},
        BoxError,
    },
    aws_sdk_dynamodb::types::AttributeValue,
    aws_sdk_s3::{operation::head_object::HeadObjectError, primitives::ByteStream},
    aws_smithy_runtime_api::client::result::SdkError,
    base64::prelude::*,
    bytes::{BufMut, Bytes, BytesMut},
    futures_util::StreamExt,
    http::Extensions,
    log::*,
    reqwest::{
        header::{HeaderMap, HeaderValue},
        Method, StatusCode, Url, Version,
    },
    sha2::{Digest, Sha256},
    std::{
        error::Error,
        fmt::{Display, Formatter, Result as FmtResult},
        str::Utf8Error,
    },
    uuid::{NoContext, Timestamp, Uuid},
};

const HEADER_CONTENT_LANGUAGE: &str = "Content-Language";
const HEADER_CONTENT_TYPE: &str = "Content-Type";

const DDB_KEY_CRAWL_ID: &str = "CrawlId";
const DDB_KEY_REQUEST_ID: &str = "RequestId";
const DDB_KEY_ORIGINAL_URL: &str = "OriginalUrl";
const DDB_KEY_FINAL_URL: &str = "FinalUrl";
const DDB_KEY_TIMESTAMP: &str = "Timestamp";
const DDB_KEY_METHOD: &str = "Method";
const DDB_KEY_STATUS_CODE: &str = "StatusCode";
const DDB_KEY_CONTENT_TYPE: &str = "ContentType";
const DDB_KEY_CONTENT_LANGUAGE: &str = "ContentLanguage";
const DDB_KEY_CONTENT_LENGTH: &str = "ContentLength";
const DDB_KEY_ETAG: &str = "Etag";
const DDB_KEY_MD5: &str = "Md5";
const DDB_KEY_S3_BUCKET: &str = "S3Bucket";
const DDB_KEY_S3_KEY: &str = "S3Key";
const DDB_KEY_SHA256: &str = "Sha256";

const INITIAL_BODY_CAPACITY: usize = 65536;

/// A Response to a submitted `Request`.
///
/// This logs the response to an S3 bucket upon creation.
#[derive(Debug)]
pub struct Response {
    /// The response's status
    status: StatusCode,

    /// The response's version
    version: Version,

    /// The response's headers
    headers: HeaderMap<HeaderValue>,

    /// The response's extensions
    extensions: Extensions,

    /// The URL of the response
    url: Url,

    /// The body of the response.
    body: Bytes,

    /// The size of the body.
    content_length: usize,
}

/// Error returned when an HTTP status code is not in the 200-399 range.
#[derive(Debug)]
pub struct HttpStatusError {
    /// The status code that was returned.
    pub status: StatusCode,

    /// The URL that was requested.
    pub url: Url,
}

impl Display for HttpStatusError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "HTTP request to {} failed: status code {}", self.url, self.status)
    }
}

impl Error for HttpStatusError {}

impl Response {
    /// Create a new [`Response`] that wraps a Reqwest [response][reqwest::Response]
    /// and tracks other metadata about this crawl.
    pub async fn new(
        resp: reqwest::Response,
        crawl_id: String,
        method: Method,
        orig_url: Url,
        log_config: Option<LogConfig>,
    ) -> Result<Self, BoxError> {
        let status = resp.status();
        let version = resp.version();
        let headers = resp.headers().clone();
        let extensions = resp.extensions().clone();
        let final_url = resp.url().clone();
        let timestamp = Timestamp::now(NoContext);
        let (timestamp_secs, timestamp_nanos) = timestamp.to_unix();
        let request_id = Uuid::new_v7(timestamp);
        let mut body = BytesMut::with_capacity(INITIAL_BODY_CAPACITY);

        // let mut body = BytesMut::with_capacity(INITIAL_BODY_CAPACITY);
        let mut stream = resp.bytes_stream();
        let mut sha256 = Sha256::new();
        let mut md5 = md5::Context::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            body.put_slice(&chunk);
            sha256.update(&chunk);
            md5.consume(&chunk);
        }

        let body = body.freeze();
        let content_length = body.len();

        let sha256 = sha256.finalize();
        let sha256_str = hex::encode(sha256.as_slice());
        let sha256_b64 = BASE64_STANDARD.encode(sha256.as_slice());

        let md5 = *md5.compute();
        let md5_str = BASE64_STANDARD.encode(md5);

        debug!("HTTP: {orig_url} status {status}, content-length {content_length}, sha256 {sha256_str}");

        if let Some(log_config) = log_config {
            let bucket = log_config.s3_bucket.clone();
            let key = format!("{}{}", log_config.s3_prefix, sha256_str);

            // Does a body with this SHA256 checksum already exist?
            let etag = match log_aws_err(
                log_config.s3_client.head_object().bucket(bucket.clone()).key(key.clone()).send().await,
                &format!("HeadObject on s3://{bucket}/{key}"),
            ) {
                Ok(head_object) => head_object.e_tag.unwrap(),
                Err(e) => {
                    let SdkError::ServiceError(ref service_error) = e else {
                        return Err(Box::new(e));
                    };

                    let HeadObjectError::NotFound(_) = service_error.err() else {
                        return Err(Box::new(e));
                    };

                    // No; write it out.
                    let bytestream = ByteStream::from(body.clone());

                    debug!("Logging to S3: s3://{bucket}/{key}");
                    debug!("MD5: {md5_str}");
                    debug!("SHA256: {sha256_str} {sha256_b64}");

                    let put_object = match log_aws_err(
                        log_config
                            .s3_client
                            .put_object()
                            .bucket(bucket.clone())
                            .key(key.clone())
                            .content_md5(md5_str.clone())
                            .checksum_sha256(sha256_b64.clone())
                            .body(bytestream)
                            .send()
                            .await,
                        &format!("PutObject s3://{bucket}/{key}"),
                    ) {
                        Ok(put_object) => put_object,
                        Err(e) => {
                            if let aws_smithy_runtime_api::client::result::SdkError::ServiceError(e2) = &e {
                                let metadata = e2.err().meta();
                                error!(
                                    "Error info: code={:?} message={:?} request_id={:?}",
                                    metadata.code(),
                                    metadata.message(),
                                    metadata.extra("request_id")
                                );
                            }

                            Err(e)?
                        }
                    };

                    put_object.e_tag.unwrap()
                }
            };

            // Write this to DynamoDB.
            let mut put_item = log_config
                .ddb_client
                .put_item()
                .table_name(log_config.ddb_table)
                .item(DDB_KEY_CRAWL_ID, AttributeValue::S(crawl_id.clone()))
                .item(DDB_KEY_REQUEST_ID, AttributeValue::S(request_id.to_string()))
                .item(DDB_KEY_FINAL_URL, AttributeValue::S(final_url.to_string()))
                .item(DDB_KEY_ORIGINAL_URL, AttributeValue::S(orig_url.to_string()))
                .item(DDB_KEY_METHOD, AttributeValue::S(method.to_string()))
                .item(DDB_KEY_SHA256, AttributeValue::S(sha256_str.clone()))
                .item(DDB_KEY_MD5, AttributeValue::S(md5_str.clone()))
                .item(DDB_KEY_ETAG, AttributeValue::S(etag.clone()))
                .item(DDB_KEY_CONTENT_LENGTH, AttributeValue::N(content_length.to_string()))
                .item(DDB_KEY_S3_BUCKET, AttributeValue::S(log_config.s3_bucket.clone()))
                .item(DDB_KEY_S3_KEY, AttributeValue::S(key))
                .item(DDB_KEY_STATUS_CODE, AttributeValue::N(status.as_u16().to_string()))
                .item(DDB_KEY_TIMESTAMP, AttributeValue::N(format!("{timestamp_secs}.{timestamp_nanos:09}")));

            if let Some(content_type) = headers.get(HEADER_CONTENT_TYPE) {
                put_item =
                    put_item.item(DDB_KEY_CONTENT_TYPE, AttributeValue::S(content_type.to_str().unwrap().to_string()));
            }

            if let Some(content_language) = headers.get(HEADER_CONTENT_LANGUAGE) {
                put_item = put_item
                    .item(DDB_KEY_CONTENT_LANGUAGE, AttributeValue::S(content_language.to_str().unwrap().to_string()));
            }

            log_aws_err(put_item.send().await, "PutItem")?;

            info!("Logged response to S3 and DynamoDB: crawl_id={crawl_id}, request_id={request_id}");
        }

        Ok(Response {
            status,
            version,
            headers,
            extensions,
            url: final_url,
            body,
            content_length,
        })
    }

    /// Get the `StatusCode` of this `Response`.
    #[inline(always)]
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Get the HTTP `Version` of this `Response`.
    #[inline(always)]
    pub fn version(&self) -> Version {
        self.version
    }

    /// Get the `Headers` of this `Response`.
    #[inline(always)]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Get a mutable reference to the `Headers` of this `Response`.
    #[inline(always)]
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.headers
    }

    /// Get the content-length of this response.
    #[inline(always)]
    pub fn content_length(&self) -> usize {
        self.content_length
    }

    /// Get the final `Url` of this `Response`.
    #[inline(always)]
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// Returns a reference to the associated extensions.
    #[inline(always)]
    pub fn extensions(&self) -> &http::Extensions {
        &self.extensions
    }

    /// Returns a mutable reference to the associated extensions.
    #[inline(always)]
    pub fn extensions_mut(&mut self) -> &mut http::Extensions {
        &mut self.extensions
    }

    /// Get the full response text.
    ///
    /// This method decodes the response body with BOM sniffing
    /// and with malformed sequences replaced with the REPLACEMENT CHARACTER.
    /// Encoding is determined from the `charset` parameter of `Content-Type` header,
    /// and defaults to `utf-8` if not presented.
    ///
    /// Note that the BOM is stripped from the returned String.
    ///
    /// # Note
    ///
    /// If the `charset` feature is disabled the method will only attempt to decode the
    /// response as UTF-8, regardless of the given `Content-Type`
    #[inline(always)]
    pub fn text(&self) -> Result<&str, Utf8Error> {
        std::str::from_utf8(&self.body)
    }

    /// Get the full response body as `Bytes`.
    #[inline(always)]
    pub fn bytes(&self) -> Bytes {
        self.body.clone()
    }

    /// Turn a response into an error if the server returned an error.
    pub fn error_for_status(self) -> Result<Self, HttpStatusError> {
        let status = self.status();
        if status.is_client_error() || status.is_server_error() {
            Err(HttpStatusError {
                status,
                url: self.url().clone(),
            })
        } else {
            Ok(self)
        }
    }
}
