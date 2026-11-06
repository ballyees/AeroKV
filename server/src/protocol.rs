use crate::auth::Authenticator;
use crate::shard::ShardPool;
use bytes::Bytes;
use http::{Method, Request, Response, StatusCode};
use http_body_util::Full;
use std::sync::Arc;
use std::time::Duration;



#[derive(Clone)]
pub struct RequestHandler {
    auth: Authenticator,
    shard_pool: ShardPool,
}

impl RequestHandler {
    pub fn new(auth: Authenticator, shard_pool: ShardPool) -> Self {
        Self {
            auth,
            shard_pool,
        }
    }

    fn check_auth<B>(&self, req: &Request<B>) -> bool {
        if !self.auth.is_enabled() {
            return true;
        }

        if let Some(val) = req.headers().get("X-AeroKV-Token") {
            if self.auth.verify(val.as_bytes()) {
                return true;
            }
        }

        if let Some(val) = req.headers().get("Authorization") {
            if let Ok(s) = val.to_str() {
                if let Some(token) = s.strip_prefix("Bearer ") {
                    if self.auth.verify(token.as_bytes()) {
                        return true;
                    }
                }
            }
        }

        false
    }

    pub async fn handle_request(
        self: Arc<Self>,
        req: Request<Bytes>,
    ) -> Response<Full<Bytes>> {
        if !self.check_auth(&req) {
            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(Full::new(Bytes::from("Unauthorized\n")))
                .unwrap();
        }

        let method = req.method().clone();
        let path = req.uri().path().to_string();
        let query = req.uri().query().unwrap_or("").to_string();

        match (&method, path.as_str()) {
            (&Method::GET, "/ping") => Response::builder()
                .status(StatusCode::OK)
                .body(Full::new(Bytes::from("PONG\n")))
                .unwrap(),

            (&Method::GET, "/stats") => {
                let stats = self.shard_pool.stats().await;
                let body = serde_json::to_vec(&stats).unwrap_or_default();
                Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "application/json")
                    .body(Full::new(Bytes::from(body)))
                    .unwrap()
            }

            (&Method::POST, "/flush") => {
                self.shard_pool.flush().await;
                Response::builder()
                    .status(StatusCode::OK)
                    .body(Full::new(Bytes::from("OK\n")))
                    .unwrap()
            }

            (&Method::POST, "/mget") => {
                let keys = parse_length_prefixed_list(req.body());
                let vals = self.shard_pool.mget(keys).await;
                let encoded = encode_length_prefixed_options(&vals);
                Response::builder()
                    .status(StatusCode::OK)
                    .body(Full::new(encoded))
                    .unwrap()
            }

            (&Method::POST, "/mset") => {
                let ttl = parse_ttl_query(&query);
                let pairs = parse_length_prefixed_pairs(req.body());
                let written = self.shard_pool.mset(pairs, ttl).await;
                Response::builder()
                    .status(StatusCode::OK)
                    .body(Full::new(Bytes::from(written.to_string())))
                    .unwrap()
            }

            _ => {
                if let Some(key_str) = path.strip_prefix("/kv/") {
                    self.handle_kv_command(&method, key_str, &query, req.into_body()).await
                } else {
                    Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Full::new(Bytes::from("Not Found\n")))
                        .unwrap()
                }
            }
        }
    }

    async fn handle_kv_command(
        &self,
        method: &Method,
        key_str: &str,
        query: &str,
        body: Bytes,
    ) -> Response<Full<Bytes>> {
        if let Some(sub) = key_str.strip_suffix("/ttl") {
            let key = Bytes::copy_from_slice(sub.as_bytes());
            let remaining = self.shard_pool.ttl(key).await;
            return Response::builder()
                .status(StatusCode::OK)
                .body(Full::new(Bytes::from(remaining.to_string())))
                .unwrap();
        }

        if let Some(sub) = key_str.strip_suffix("/exists") {
            let key = Bytes::copy_from_slice(sub.as_bytes());
            let exists = self.shard_pool.exists(key).await;
            let status = if exists { StatusCode::OK } else { StatusCode::NOT_FOUND };
            let val = if exists { "1" } else { "0" };
            return Response::builder()
                .status(status)
                .body(Full::new(Bytes::from(val)))
                .unwrap();
        }

        if let Some(sub) = key_str.strip_suffix("/incr") {
            let key = Bytes::copy_from_slice(sub.as_bytes());
            let delta = parse_query_param(query, "delta")
                .and_then(|d| d.parse::<i64>().ok())
                .unwrap_or(1);
            return match self.shard_pool.incr(key, delta).await {
                Ok(new_val) => Response::builder()
                    .status(StatusCode::OK)
                    .body(Full::new(Bytes::from(new_val.to_string())))
                    .unwrap(),
                Err(e) => Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Full::new(Bytes::from(e)))
                    .unwrap(),
            };
        }

        let key = Bytes::copy_from_slice(key_str.as_bytes());

        match *method {
            Method::GET => match self.shard_pool.get(key).await {
                Some(val) => Response::builder()
                    .status(StatusCode::OK)
                    .body(Full::new(val))
                    .unwrap(),
                None => Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Full::new(Bytes::from("Key not found\n")))
                    .unwrap(),
            },

            Method::PUT => {
                let ttl = parse_ttl_query(query);
                let nx = parse_query_param(query, "nx")
                    .map(|v| v == "1" || v == "true")
                    .unwrap_or(false);

                let set_success = self.shard_pool.set(key, body, ttl, nx).await;
                if set_success {
                    let status = if nx { StatusCode::CREATED } else { StatusCode::OK };
                    Response::builder()
                        .status(status)
                        .body(Full::new(Bytes::from("OK\n")))
                        .unwrap()
                } else {
                    Response::builder()
                        .status(StatusCode::OK)
                        .body(Full::new(Bytes::from("0\n")))
                        .unwrap()
                }
            }

            Method::DELETE => {
                let deleted = self.shard_pool.delete(key).await;
                let body = if deleted { "1\n" } else { "0\n" };
                Response::builder()
                    .status(StatusCode::OK)
                    .body(Full::new(Bytes::from(body)))
                    .unwrap()
            }

            Method::HEAD => {
                let exists = self.shard_pool.exists(key).await;
                let status = if exists { StatusCode::OK } else { StatusCode::NOT_FOUND };
                Response::builder()
                    .status(status)
                    .body(Full::new(Bytes::new()))
                    .unwrap()
            }

            _ => Response::builder()
                .status(StatusCode::METHOD_NOT_ALLOWED)
                .body(Full::new(Bytes::from("Method Not Allowed\n")))
                .unwrap(),
        }
    }
}

fn parse_query_param<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    for part in query.split('&') {
        if let Some((k, v)) = part.split_once('=') {
            if k == key {
                return Some(v);
            }
        }
    }
    None
}

fn parse_ttl_query(query: &str) -> Option<Duration> {
    parse_query_param(query, "ttl")
        .and_then(|t| t.parse::<u64>().ok())
        .map(Duration::from_millis)
}

fn parse_length_prefixed_list(mut buf: &[u8]) -> Vec<Bytes> {
    if buf.len() < 4 {
        return Vec::new();
    }
    let count = u32::from_be_bytes(buf[..4].try_into().unwrap()) as usize;
    buf = &buf[4..];
    let mut list = Vec::with_capacity(count);

    for _ in 0..count {
        if buf.len() < 4 {
            break;
        }
        let len = u32::from_be_bytes(buf[..4].try_into().unwrap()) as usize;
        buf = &buf[4..];
        if buf.len() < len {
            break;
        }
        list.push(Bytes::copy_from_slice(&buf[..len]));
        buf = &buf[len..];
    }
    list
}

fn parse_length_prefixed_pairs(mut buf: &[u8]) -> Vec<(Bytes, Bytes)> {
    if buf.len() < 4 {
        return Vec::new();
    }
    let count = u32::from_be_bytes(buf[..4].try_into().unwrap()) as usize;
    buf = &buf[4..];
    let mut list = Vec::with_capacity(count);

    for _ in 0..count {
        if buf.len() < 4 {
            break;
        }
        let klen = u32::from_be_bytes(buf[..4].try_into().unwrap()) as usize;
        buf = &buf[4..];
        if buf.len() < klen {
            break;
        }
        let k = Bytes::copy_from_slice(&buf[..klen]);
        buf = &buf[klen..];

        if buf.len() < 4 {
            break;
        }
        let vlen = u32::from_be_bytes(buf[..4].try_into().unwrap()) as usize;
        buf = &buf[4..];
        if buf.len() < vlen {
            break;
        }
        let v = Bytes::copy_from_slice(&buf[..vlen]);
        buf = &buf[vlen..];

        list.push((k, v));
    }
    list
}

fn encode_length_prefixed_options(items: &[Option<Bytes>]) -> Bytes {
    let mut total_len = 4;
    for item in items {
        total_len += 1;
        if let Some(b) = item {
            total_len += 4 + b.len();
        }
    }

    let mut out = Vec::with_capacity(total_len);
    out.extend_from_slice(&(items.len() as u32).to_be_bytes());
    for item in items {
        match item {
            Some(b) => {
                out.push(1);
                out.extend_from_slice(&(b.len() as u32).to_be_bytes());
                out.extend_from_slice(b);
            }
            None => {
                out.push(0);
            }
        }
    }
    Bytes::from(out)
}
