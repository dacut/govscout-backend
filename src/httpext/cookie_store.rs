//! A fork of [`reqwest_cookie_store`](https://docs.rs/reqwest_cookie_store) that serializes
//! non-persistent cookies by default.
use {
    bytes::Bytes,
    cookie_store::{CookieStore as BaseCookieStore, RawCookie, RawCookieParseError},
    reqwest::{header::HeaderValue, Url},
    serde::{
        de::{SeqAccess, Visitor},
        Deserialize, Deserializer, Serialize, Serializer,
    },
    std::{
        fmt::{Formatter, Result as FmtResult},
        iter,
        ops::{Deref, DerefMut},
        sync::RwLock,
    },
};

/// A cookie store that can be serialized and deserialized across requests.
/// 
/// This is a variant of `reqwest_cookie_store::CookieStore` that implements `Serialize` and
/// `Deserialize` differently, allowing non-persistent cookies to be serialized. This is necessary
/// because our virtual browsing session spans multiple Lambda requests.
#[derive(Clone, Debug, Default)]
pub struct CookieStore(BaseCookieStore);

impl Deref for CookieStore {
    type Target = BaseCookieStore;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for CookieStore {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<BaseCookieStore> for CookieStore {
    fn from(store: BaseCookieStore) -> Self {
        CookieStore(store)
    }
}

struct CookieStoreVisitor;

impl<'de> Visitor<'de> for CookieStoreVisitor {
    type Value = CookieStore;

    fn expecting(&self, f: &mut Formatter) -> FmtResult {
        f.write_str("a sequence of cookies")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        Ok(CookieStore(BaseCookieStore::from_cookies(iter::from_fn(|| seq.next_element().transpose()), false)?))
    }
}

impl<'de> Deserialize<'de> for CookieStore {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(CookieStoreVisitor)
    }
}

impl Serialize for CookieStore {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_seq(self.iter_unexpired())
    }
}

/// A [`CookieStore`] wrapped in a read-write lock.
/// 
/// This allows the read-write lockked cookie store to implement the
/// Reqwest [`CookieStore`][reqwest::cookie::CookieStore] trait.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct CookieStoreRwLock(RwLock<CookieStore>);

impl From<BaseCookieStore> for CookieStoreRwLock {
    fn from(store: BaseCookieStore) -> Self {
        CookieStoreRwLock(RwLock::new(store.into()))
    }
}

impl From<CookieStore> for CookieStoreRwLock {
    fn from(store: CookieStore) -> Self {
        CookieStoreRwLock(RwLock::new(store))
    }
}

impl Deref for CookieStoreRwLock {
    type Target = RwLock<CookieStore>;
    fn deref(&self) -> &RwLock<CookieStore> {
        &self.0
    }
}

impl DerefMut for CookieStoreRwLock {
    fn deref_mut(&mut self) -> &mut RwLock<CookieStore> {
        &mut self.0
    }
}

impl reqwest::cookie::CookieStore for CookieStoreRwLock {
    fn set_cookies(&self, cookie_headers: &mut dyn Iterator<Item = &HeaderValue>, url: &Url) {
        let mut write = self.0.write().unwrap();
        set_cookies(&mut write, cookie_headers, url);
    }

    fn cookies(&self, url: &Url) -> Option<HeaderValue> {
        let read = self.0.read().unwrap();
        cookies(&read, url)
    }
}

fn set_cookies(cookie_store: &mut CookieStore, cookie_headers: &mut dyn Iterator<Item = &HeaderValue>, url: &Url) {
    let cookies = cookie_headers.filter_map(|val| {
        std::str::from_utf8(val.as_bytes())
            .map_err(RawCookieParseError::from)
            .and_then(RawCookie::parse)
            .map(|c| c.into_owned())
            .ok()
    });
    cookie_store.store_response_cookies(cookies, url);
}

fn cookies(cookie_store: &CookieStore, url: &Url) -> Option<HeaderValue> {
    let s = cookie_store
        .get_request_values(url)
        .map(|(name, value)| format!("{}={}", name, value))
        .collect::<Vec<_>>()
        .join("; ");

    if s.is_empty() {
        return None;
    }

    HeaderValue::from_maybe_shared(Bytes::from(s)).ok()
}
