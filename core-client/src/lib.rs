use bytes::Bytes;
use http::{HeaderMap, HeaderValue, Method, Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use hyper_util::client::legacy::Client as HyperLegacyClient;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum AeroError {
    #[error("Network I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Hyper error: {0}")]
    Hyper(#[from] hyper::Error),
    #[error("HTTP error: {0}")]
    Http(#[from] http::Error),
    #[error("AeroKV server returned status: {0}")]
    ServerStatus(StatusCode),
    #[error("Key not found")]
    NotFound,
    #[error("Operation failed: {0}")]
    Custom(String),
}

pub struct AeroClient {
    host: String,
    port: u16,
    token: Option<String>,
    key_prefix: Option<String>,
    tcp_client: HyperLegacyClient<HttpConnector, Full<Bytes>>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct ShardStatsSummary {
    pub key_count: usize,
    pub used_memory_bytes: usize,
    pub hits: u64,
    pub misses: u64,
    pub expired_keys: u64,
    pub evicted_keys: u64,
}

impl AeroClient {
    pub async fn connect(host: &str, port: u16, token: Option<&str>) -> Result<Arc<Self>, AeroError> {
        Self::connect_with_prefix(host, port, token, None).await
    }

    pub async fn connect_with_prefix(
        host: &str,
        port: u16,
        token: Option<&str>,
        prefix: Option<&str>,
    ) -> Result<Arc<Self>, AeroError> {
        let mut http_connector = HttpConnector::new();
        http_connector.set_nodelay(true);
        http_connector.set_keepalive(Some(Duration::from_secs(60)));
        let tcp_client = HyperLegacyClient::builder(TokioExecutor::new())
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(256)
            .build(http_connector);

        Ok(Arc::new(Self {
            host: host.to_string(),
            port,
            token: token.map(|t| t.to_string()),
            key_prefix: prefix.filter(|p| !p.is_empty()).map(|p| p.to_string()),
            tcp_client,
        }))
    }

    pub fn with_prefix(&self, prefix: &str) -> Arc<Self> {
        Arc::new(Self {
            host: self.host.clone(),
            port: self.port,
            token: self.token.clone(),
            key_prefix: if prefix.is_empty() {
                None
            } else {
                Some(prefix.to_string())
            },
            tcp_client: self.tcp_client.clone(),
        })
    }

    pub fn prefix(&self) -> Option<&str> {
        self.key_prefix.as_deref()
    }

    fn format_key(&self, key: &str) -> String {
        match &self.key_prefix {
            Some(prefix) => format!("{}{}", prefix, key),
            None => key.to_string(),
        }
    }

    pub fn active_protocol(&self) -> &'static str {
        "http"
    }

    fn apply_auth_headers(&self, headers: &mut HeaderMap) {
        if let Some(ref t) = self.token {
            headers.insert("X-AeroKV-Token", HeaderValue::from_str(t).unwrap());
        }
    }

    async fn send_request(
        &self,
        method: Method,
        path_and_query: &str,
        body_bytes: Bytes,
    ) -> Result<Response<Bytes>, AeroError> {
        let uri_str = format!("http://{}:{}{}", self.host, self.port, path_and_query);
        let uri: http::Uri = uri_str.parse()
            .map_err(|e| AeroError::Custom(format!("URI parse error: {e}")))?;

        let mut req = Request::builder()
            .method(method)
            .uri(uri);

        self.apply_auth_headers(req.headers_mut().unwrap());
        let req = req.body(Full::new(body_bytes)).unwrap();

        let resp = self.tcp_client.request(req).await
            .map_err(|e| AeroError::Custom(format!("HTTP request failed: {e}")))?;

        let (parts, body) = resp.into_parts();
        let bytes = body.collect().await
            .map_err(|e| AeroError::Custom(format!("Body collect failed: {e}")))?
            .to_bytes();
        Ok(Response::from_parts(parts, bytes))
    }

    pub async fn get(&self, key: &str) -> Result<Option<Bytes>, AeroError> {
        let k = self.format_key(key);
        let resp = self.send_request(Method::GET, &format!("/kv/{}", k), Bytes::new()).await?;
        if resp.status() == StatusCode::OK {
            Ok(Some(resp.into_body()))
        } else if resp.status() == StatusCode::NOT_FOUND {
            Ok(None)
        } else {
            Err(AeroError::ServerStatus(resp.status()))
        }
    }

    pub async fn set(&self, key: &str, val: Bytes, ttl: Option<Duration>) -> Result<(), AeroError> {
        let k = self.format_key(key);
        let path = match ttl {
            Some(t) => format!("/kv/{}?ttl={}", k, t.as_millis()),
            None => format!("/kv/{}", k),
        };
        let resp = self.send_request(Method::PUT, &path, val).await?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(AeroError::ServerStatus(resp.status()))
        }
    }

    pub async fn setnx(&self, key: &str, val: Bytes, ttl: Option<Duration>) -> Result<bool, AeroError> {
        let k = self.format_key(key);
        let path = match ttl {
            Some(t) => format!("/kv/{}?nx=1&ttl={}", k, t.as_millis()),
            None => format!("/kv/{}?nx=1", k),
        };
        let resp = self.send_request(Method::PUT, &path, val).await?;
        if resp.status() == StatusCode::CREATED {
            Ok(true)
        } else if resp.status() == StatusCode::OK {
            let body = resp.into_body();
            Ok(body.starts_with(b"OK"))
        } else {
            Err(AeroError::ServerStatus(resp.status()))
        }
    }

    pub async fn delete(&self, key: &str) -> Result<bool, AeroError> {
        let k = self.format_key(key);
        let resp = self.send_request(Method::DELETE, &format!("/kv/{}", k), Bytes::new()).await?;
        if resp.status().is_success() {
            let body = resp.into_body();
            Ok(body.starts_with(b"1"))
        } else {
            Err(AeroError::ServerStatus(resp.status()))
        }
    }

    pub async fn incr(&self, key: &str, delta: i64) -> Result<i64, AeroError> {
        let k = self.format_key(key);
        let path = format!("/kv/{}/incr?delta={}", k, delta);
        let resp = self.send_request(Method::POST, &path, Bytes::new()).await?;
        if resp.status().is_success() {
            let body = resp.into_body();
            let s = std::str::from_utf8(&body)
                .map_err(|e| AeroError::Custom(e.to_string()))?;
            let val = s.trim().parse::<i64>()
                .map_err(|e| AeroError::Custom(e.to_string()))?;
            Ok(val)
        } else {
            Err(AeroError::ServerStatus(resp.status()))
        }
    }

    pub async fn ttl(&self, key: &str) -> Result<i64, AeroError> {
        let k = self.format_key(key);
        let resp = self.send_request(Method::GET, &format!("/kv/{}/ttl", k), Bytes::new()).await?;
        if resp.status().is_success() {
            let body = resp.into_body();
            let s = std::str::from_utf8(&body)
                .map_err(|e| AeroError::Custom(e.to_string()))?;
            let val = s.trim().parse::<i64>()
                .map_err(|e| AeroError::Custom(e.to_string()))?;
            Ok(val)
        } else {
            Err(AeroError::ServerStatus(resp.status()))
        }
    }

    pub async fn exists(&self, key: &str) -> Result<bool, AeroError> {
        let k = self.format_key(key);
        let resp = self.send_request(Method::GET, &format!("/kv/{}/exists", k), Bytes::new()).await?;
        Ok(resp.status() == StatusCode::OK)
    }

    pub async fn ping(&self) -> Result<bool, AeroError> {
        let resp = self.send_request(Method::GET, "/ping", Bytes::new()).await?;
        Ok(resp.status() == StatusCode::OK)
    }

    pub async fn stats(&self) -> Result<ShardStatsSummary, AeroError> {
        let resp = self.send_request(Method::GET, "/stats", Bytes::new()).await?;
        if resp.status().is_success() {
            let stats = serde_json::from_slice(&resp.into_body())
                .map_err(|e| AeroError::Custom(e.to_string()))?;
            Ok(stats)
        } else {
            Err(AeroError::ServerStatus(resp.status()))
        }
    }

    pub async fn flush(&self) -> Result<(), AeroError> {
        let resp = self.send_request(Method::POST, "/flush", Bytes::new()).await?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(AeroError::ServerStatus(resp.status()))
        }
    }

    pub async fn mget(&self, keys: &[&str]) -> Result<Vec<Option<Bytes>>, AeroError> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&(keys.len() as u32).to_be_bytes());
        for k in keys {
            let formatted = self.format_key(k);
            let b = formatted.as_bytes();
            buf.extend_from_slice(&(b.len() as u32).to_be_bytes());
            buf.extend_from_slice(b);
        }

        let resp = self.send_request(Method::POST, "/mget", Bytes::from(buf)).await?;
        if !resp.status().is_success() {
            return Err(AeroError::ServerStatus(resp.status()));
        }

        let mut data = resp.into_body();
        if data.len() < 4 {
            return Ok(Vec::new());
        }

        let count = u32::from_be_bytes(data[..4].try_into().unwrap()) as usize;
        data = data.slice(4..);

        let mut results = Vec::with_capacity(count);
        for _ in 0..count {
            if data.is_empty() {
                break;
            }
            let found = data[0] == 1;
            data = data.slice(1..);
            if found {
                if data.len() < 4 {
                    break;
                }
                let len = u32::from_be_bytes(data[..4].try_into().unwrap()) as usize;
                data = data.slice(4..);
                if data.len() < len {
                    break;
                }
                results.push(Some(data.slice(..len)));
                data = data.slice(len..);
            } else {
                results.push(None);
            }
        }

        Ok(results)
    }

    pub async fn mset(&self, entries: &[(&str, Bytes)], ttl: Option<Duration>) -> Result<usize, AeroError> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&(entries.len() as u32).to_be_bytes());
        for (k, v) in entries {
            let formatted = self.format_key(k);
            let kb = formatted.as_bytes();
            buf.extend_from_slice(&(kb.len() as u32).to_be_bytes());
            buf.extend_from_slice(kb);
            buf.extend_from_slice(&(v.len() as u32).to_be_bytes());
            buf.extend_from_slice(v);
        }

        let path = match ttl {
            Some(t) => format!("/mset?ttl={}", t.as_millis()),
            None => "/mset".to_string(),
        };

        let resp = self.send_request(Method::POST, &path, Bytes::from(buf)).await?;
        if resp.status().is_success() {
            let body = resp.into_body();
            let s = std::str::from_utf8(&body)
                .map_err(|e| AeroError::Custom(e.to_string()))?;
            let written = s.trim().parse::<usize>()
                .map_err(|e| AeroError::Custom(e.to_string()))?;
            Ok(written)
        } else {
            Err(AeroError::ServerStatus(resp.status()))
        }
    }
}

// C-ABI FFI Layer
pub struct FfiAeroClient {
    pub client: Arc<AeroClient>,
    pub rt: tokio::runtime::Runtime,
}

/// # Safety
/// `host` must be a valid null-terminated C string. `token` and `prefix` may be null or valid null-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn aerokv_client_new_with_prefix(
    host: *const std::os::raw::c_char,
    port: u16,
    token: *const std::os::raw::c_char,
    prefix: *const std::os::raw::c_char,
) -> *mut FfiAeroClient {
    if host.is_null() {
        return std::ptr::null_mut();
    }
    let host_str = std::ffi::CStr::from_ptr(host).to_string_lossy().into_owned();
    let token_str = if !token.is_null() {
        Some(std::ffi::CStr::from_ptr(token).to_string_lossy().into_owned())
    } else {
        None
    };
    let prefix_str = if !prefix.is_null() {
        Some(std::ffi::CStr::from_ptr(prefix).to_string_lossy().into_owned())
    } else {
        None
    };

    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(_) => return std::ptr::null_mut(),
    };

    match rt.block_on(AeroClient::connect_with_prefix(&host_str, port, token_str.as_deref(), prefix_str.as_deref())) {
        Ok(c) => Box::into_raw(Box::new(FfiAeroClient { client: c, rt })),
        Err(_) => std::ptr::null_mut(),
    }
}

/// # Safety
/// `host` must be a valid null-terminated C string. `token` may be null or a valid null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn aerokv_client_new(
    host: *const std::os::raw::c_char,
    port: u16,
    token: *const std::os::raw::c_char,
) -> *mut FfiAeroClient {
    aerokv_client_new_with_prefix(host, port, token, std::ptr::null())
}

/// # Safety
/// `client` must be a pointer returned by `aerokv_client_new` or null.
#[no_mangle]
pub unsafe extern "C" fn aerokv_client_free(client: *mut FfiAeroClient) {
    if !client.is_null() {
        let _ = Box::from_raw(client);
    }
}

/// # Safety
/// `client` and `key` must be valid non-null pointers.
#[no_mangle]
pub unsafe extern "C" fn aerokv_client_get(
    client: *mut FfiAeroClient,
    key: *const std::os::raw::c_char,
    out_len: *mut usize,
) -> *mut u8 {
    if client.is_null() || key.is_null() || out_len.is_null() {
        return std::ptr::null_mut();
    }
    let ffi = &*client;
    let key_str = std::ffi::CStr::from_ptr(key).to_string_lossy();

    match ffi.rt.block_on(ffi.client.get(&key_str)) {
        Ok(Some(bytes)) => {
            *out_len = bytes.len();
            let mut v = bytes.to_vec();
            let ptr = v.as_mut_ptr();
            std::mem::forget(v);
            ptr
        }
        _ => {
            *out_len = 0;
            std::ptr::null_mut()
        }
    }
}

/// # Safety
/// `client`, `key`, and `val` must be valid non-null pointers.
#[no_mangle]
pub unsafe extern "C" fn aerokv_client_set(
    client: *mut FfiAeroClient,
    key: *const std::os::raw::c_char,
    val: *const u8,
    val_len: usize,
    ttl_ms: u64,
) -> bool {
    if client.is_null() || key.is_null() || (val.is_null() && val_len > 0) {
        return false;
    }
    let ffi = &*client;
    let key_str = std::ffi::CStr::from_ptr(key).to_string_lossy();
    let bytes = if val_len > 0 {
        Bytes::copy_from_slice(std::slice::from_raw_parts(val, val_len))
    } else {
        Bytes::new()
    };
    let ttl = if ttl_ms > 0 {
        Some(Duration::from_millis(ttl_ms))
    } else {
        None
    };

    ffi.rt.block_on(ffi.client.set(&key_str, bytes, ttl)).is_ok()
}

/// # Safety
/// `client` and `key` must be valid non-null pointers.
#[no_mangle]
pub unsafe extern "C" fn aerokv_client_delete(
    client: *mut FfiAeroClient,
    key: *const std::os::raw::c_char,
) -> bool {
    if client.is_null() || key.is_null() {
        return false;
    }
    let ffi = &*client;
    let key_str = std::ffi::CStr::from_ptr(key).to_string_lossy();
    ffi.rt.block_on(ffi.client.delete(&key_str)).unwrap_or(false)
}

/// # Safety
/// `client` and `key` must be valid non-null pointers.
#[no_mangle]
pub unsafe extern "C" fn aerokv_client_incr(
    client: *mut FfiAeroClient,
    key: *const std::os::raw::c_char,
    delta: i64,
    out_val: *mut i64,
) -> bool {
    if client.is_null() || key.is_null() || out_val.is_null() {
        return false;
    }
    let ffi = &*client;
    let key_str = std::ffi::CStr::from_ptr(key).to_string_lossy();
    match ffi.rt.block_on(ffi.client.incr(&key_str, delta)) {
        Ok(v) => {
            *out_val = v;
            true
        }
        Err(_) => false,
    }
}

/// # Safety
/// `ptr` must be a pointer returned by `aerokv_client_get` or null.
#[no_mangle]
pub unsafe extern "C" fn aerokv_bytes_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        let _ = Vec::from_raw_parts(ptr, len, len);
    }
}
