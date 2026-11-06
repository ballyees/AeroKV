use bytes::Bytes;
use fxhash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ShardStats {
    pub key_count: usize,
    pub used_memory_bytes: usize,
    pub hits: u64,
    pub misses: u64,
    pub expired_keys: u64,
    pub evicted_keys: u64,
}

pub struct CacheValue {
    pub data: Bytes,
    pub expires_at: Option<Instant>,
    pub last_accessed_tick: u64,
}

pub enum BatchOp {
    Get(Bytes),
    Set {
        key: Bytes,
        val: Bytes,
        ttl: Option<Duration>,
        nx: bool,
    },
    Delete(Bytes),
    Incr {
        key: Bytes,
        delta: i64,
    },
    Ttl(Bytes),
    Exists(Bytes),
}

pub enum BatchResult {
    Value(Option<Bytes>),
    Bool(bool),
    Int(i64),
    Error(&'static str),
}

pub enum ShardCommand {
    Get {
        key: Bytes,
        resp: oneshot::Sender<Option<Bytes>>,
    },
    Set {
        key: Bytes,
        val: Bytes,
        ttl: Option<Duration>,
        nx: bool,
        resp: oneshot::Sender<bool>,
    },
    Delete {
        key: Bytes,
        resp: oneshot::Sender<bool>,
    },
    Incr {
        key: Bytes,
        delta: i64,
        resp: oneshot::Sender<Result<i64, &'static str>>,
    },
    Ttl {
        key: Bytes,
        resp: oneshot::Sender<i64>,
    },
    Exists {
        key: Bytes,
        resp: oneshot::Sender<bool>,
    },
    Batch {
        ops: Vec<BatchOp>,
        resp: oneshot::Sender<Vec<BatchResult>>,
    },
    Stats {
        resp: oneshot::Sender<ShardStats>,
    },
    Flush {
        resp: oneshot::Sender<()>,
    },
}

pub struct LocalShard {
    store: FxHashMap<Bytes, CacheValue>,
    expiration_timeline: BTreeMap<Instant, Vec<Bytes>>,
    max_memory_bytes: usize,
    used_memory_bytes: usize,
    tick: u64,
    hits: u64,
    misses: u64,
    expired_keys: u64,
    evicted_keys: u64,
}

impl LocalShard {
    pub fn new(max_memory_bytes: usize) -> Self {
        Self {
            store: FxHashMap::default(),
            expiration_timeline: BTreeMap::new(),
            max_memory_bytes,
            used_memory_bytes: 0,
            tick: 0,
            hits: 0,
            misses: 0,
            expired_keys: 0,
            evicted_keys: 0,
        }
    }

    #[inline]
    fn next_tick(&mut self) -> u64 {
        self.tick = self.tick.wrapping_add(1);
        self.tick
    }

    #[inline]
    fn entry_size(key: &Bytes, val: &Bytes) -> usize {
        key.len() + val.len() + std::mem::size_of::<CacheValue>() + 32
    }

    fn evict_if_needed(&mut self, required_additional_bytes: usize) {
        if self.max_memory_bytes == 0 {
            return;
        }

        while self.used_memory_bytes + required_additional_bytes > self.max_memory_bytes
            && !self.store.is_empty()
        {
            let mut sample_count = 0;
            let mut best_candidate: Option<Bytes> = None;
            let mut oldest_tick = u64::MAX;

            for (k, v) in self.store.iter() {
                if v.last_accessed_tick < oldest_tick {
                    oldest_tick = v.last_accessed_tick;
                    best_candidate = Some(k.clone());
                }
                sample_count += 1;
                if sample_count >= 5 {
                    break;
                }
            }

            if let Some(key_to_evict) = best_candidate {
                self.delete(&key_to_evict);
                self.evicted_keys += 1;
            } else {
                break;
            }
        }
    }

    pub fn set(
        &mut self,
        key: Bytes,
        val: Bytes,
        ttl: Option<Duration>,
        nx: bool,
    ) -> bool {
        let now = Instant::now();
        let current_tick = self.next_tick();

        // Check if key exists and is non-expired
        if let Some(existing) = self.store.get(&key) {
            if let Some(exp) = existing.expires_at {
                if now > exp {
                    // Stale entry, treat as not existing
                    self.delete(&key);
                } else if nx {
                    return false;
                }
            } else if nx {
                return false;
            }
        }

        let needed_bytes = Self::entry_size(&key, &val);
        self.evict_if_needed(needed_bytes);

        // If key already existed, remove old memory count
        if let Some(old) = self.store.remove(&key) {
            self.used_memory_bytes = self
                .used_memory_bytes
                .saturating_sub(Self::entry_size(&key, &old.data));
        }

        let expires_at = ttl.map(|t| now + t);
        if let Some(expiry) = expires_at {
            self.expiration_timeline
                .entry(expiry)
                .or_default()
                .push(key.clone());
        }

        self.used_memory_bytes += needed_bytes;
        self.store.insert(
            key,
            CacheValue {
                data: val,
                expires_at,
                last_accessed_tick: current_tick,
            },
        );
        true
    }

    pub fn get(&mut self, key: &Bytes) -> Option<Bytes> {
        let now = Instant::now();
        let tick = self.next_tick();

        if let Some(val) = self.store.get_mut(key) {
            if let Some(expiry) = val.expires_at {
                if now > expiry {
                    self.delete(key);
                    self.expired_keys += 1;
                    self.misses += 1;
                    return None;
                }
            }
            val.last_accessed_tick = tick;
            self.hits += 1;
            return Some(val.data.clone());
        }

        self.misses += 1;
        None
    }

    pub fn delete(&mut self, key: &Bytes) -> bool {
        if let Some(old) = self.store.remove(key) {
            self.used_memory_bytes = self
                .used_memory_bytes
                .saturating_sub(Self::entry_size(key, &old.data));
            true
        } else {
            false
        }
    }

    pub fn ttl(&mut self, key: &Bytes) -> i64 {
        let now = Instant::now();
        if let Some(val) = self.store.get(key) {
            if let Some(expiry) = val.expires_at {
                if now > expiry {
                    self.delete(key);
                    self.expired_keys += 1;
                    return -2;
                }
                let remaining = expiry.saturating_duration_since(now).as_millis() as i64;
                return remaining.max(0);
            }
            return -1; // No TTL set
        }
        -2 // Key does not exist
    }

    pub fn exists(&mut self, key: &Bytes) -> bool {
        let now = Instant::now();
        if let Some(val) = self.store.get(key) {
            if let Some(expiry) = val.expires_at {
                if now > expiry {
                    self.delete(key);
                    self.expired_keys += 1;
                    return false;
                }
            }
            return true;
        }
        false
    }

    pub fn incr(&mut self, key: &Bytes, delta: i64) -> Result<i64, &'static str> {
        let now = Instant::now();
        let current_tick = self.next_tick();

        let (current_num, existing_expiry) = if let Some(val) = self.store.get(key) {
            if let Some(expiry) = val.expires_at {
                if now > expiry {
                    self.delete(key);
                    self.expired_keys += 1;
                    (0i64, None)
                } else {
                    let s = std::str::from_utf8(&val.data)
                        .map_err(|_| "ERR value is not a valid integer")?;
                    let n = s.parse::<i64>()
                        .map_err(|_| "ERR value is not an integer")?;
                    (n, val.expires_at)
                }
            } else {
                let s = std::str::from_utf8(&val.data)
                    .map_err(|_| "ERR value is not a valid integer")?;
                let n = s.parse::<i64>()
                    .map_err(|_| "ERR value is not an integer")?;
                (n, None)
            }
        } else {
            (0i64, None)
        };

        let new_num = current_num
            .checked_add(delta)
            .ok_or("ERR increment or decrement would overflow")?;

        let val_bytes = Bytes::from(new_num.to_string());
        let needed_bytes = Self::entry_size(key, &val_bytes);
        self.evict_if_needed(needed_bytes);

        if let Some(old) = self.store.remove(key) {
            self.used_memory_bytes = self
                .used_memory_bytes
                .saturating_sub(Self::entry_size(key, &old.data));
        }

        self.used_memory_bytes += needed_bytes;
        self.store.insert(
            key.clone(),
            CacheValue {
                data: val_bytes,
                expires_at: existing_expiry,
                last_accessed_tick: current_tick,
            },
        );

        Ok(new_num)
    }

    pub fn flush(&mut self) {
        self.store.clear();
        self.expiration_timeline.clear();
        self.used_memory_bytes = 0;
    }

    pub fn stats(&self) -> ShardStats {
        ShardStats {
            key_count: self.store.len(),
            used_memory_bytes: self.used_memory_bytes,
            hits: self.hits,
            misses: self.misses,
            expired_keys: self.expired_keys,
            evicted_keys: self.evicted_keys,
        }
    }

    pub fn active_cleanup_job(&mut self) {
        let now = Instant::now();
        let mut expired_timeline = self.expiration_timeline.split_off(&(now + Duration::from_nanos(1)));
        std::mem::swap(&mut self.expiration_timeline, &mut expired_timeline);

        for (_time, keys) in expired_timeline {
            for key in keys {
                if let Some(val) = self.store.get(&key) {
                    if let Some(expiry) = val.expires_at {
                        if expiry <= now {
                            self.delete(&key);
                            self.expired_keys += 1;
                        }
                    }
                }
            }
        }
    }
}

fn execute_shard_command(shard: &mut LocalShard, cmd: ShardCommand) {
    match cmd {
        ShardCommand::Get { key, resp } => {
            let _ = resp.send(shard.get(&key));
        }
        ShardCommand::Set { key, val, ttl, nx, resp } => {
            let _ = resp.send(shard.set(key, val, ttl, nx));
        }
        ShardCommand::Delete { key, resp } => {
            let _ = resp.send(shard.delete(&key));
        }
        ShardCommand::Incr { key, delta, resp } => {
            let _ = resp.send(shard.incr(&key, delta));
        }
        ShardCommand::Ttl { key, resp } => {
            let _ = resp.send(shard.ttl(&key));
        }
        ShardCommand::Exists { key, resp } => {
            let _ = resp.send(shard.exists(&key));
        }
        ShardCommand::Batch { ops, resp } => {
            let mut results = Vec::with_capacity(ops.len());
            for op in ops {
                match op {
                    BatchOp::Get(k) => results.push(BatchResult::Value(shard.get(&k))),
                    BatchOp::Set { key, val, ttl, nx } => results.push(BatchResult::Bool(shard.set(key, val, ttl, nx))),
                    BatchOp::Delete(k) => results.push(BatchResult::Bool(shard.delete(&k))),
                    BatchOp::Incr { key, delta } => match shard.incr(&key, delta) {
                        Ok(n) => results.push(BatchResult::Int(n)),
                        Err(e) => results.push(BatchResult::Error(e)),
                    },
                    BatchOp::Ttl(k) => results.push(BatchResult::Int(shard.ttl(&k))),
                    BatchOp::Exists(k) => results.push(BatchResult::Bool(shard.exists(&k))),
                }
            }
            let _ = resp.send(results);
        }
        ShardCommand::Stats { resp } => {
            let _ = resp.send(shard.stats());
        }
        ShardCommand::Flush { resp } => {
            shard.flush();
            let _ = resp.send(());
        }
    }
}

pub async fn run_shard_worker(
    mut shard: LocalShard,
    mut rx: mpsc::Receiver<ShardCommand>,
) {
    let mut cleanup_interval = tokio::time::interval(Duration::from_millis(100));

    loop {
        tokio::select! {
            biased;
            cmd = rx.recv() => {
                match cmd {
                    Some(first_cmd) => {
                        execute_shard_command(&mut shard, first_cmd);
                        // Cooperative batch drain: process pending commands without re-entering select
                        for _ in 0..128 {
                            match rx.try_recv() {
                                Ok(next_cmd) => execute_shard_command(&mut shard, next_cmd),
                                Err(_) => break,
                            }
                        }
                    }
                    None => break, // Channel closed, terminate worker
                }
            }
            _ = cleanup_interval.tick() => {
                shard.active_cleanup_job();
            }
        }
    }
}

#[derive(Clone)]
pub struct ShardPool {
    shards: Vec<mpsc::Sender<ShardCommand>>,
}

impl ShardPool {
    pub fn new(shards: Vec<mpsc::Sender<ShardCommand>>) -> Self {
        Self { shards }
    }

    #[inline]
    pub fn shard_count(&self) -> usize {
        self.shards.len()
    }



    #[inline]
    pub fn shard_idx(&self, key: &[u8]) -> usize {
        (fxhash::hash64(key) as usize) % self.shards.len()
    }

    pub async fn execute_batch(&self, shard_idx: usize, ops: Vec<BatchOp>) -> Vec<BatchResult> {
        let (tx, rx) = oneshot::channel();
        if self.shards[shard_idx].send(ShardCommand::Batch { ops, resp: tx }).await.is_ok() {
            rx.await.unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    pub async fn get(&self, key: Bytes) -> Option<Bytes> {
        let idx = self.shard_idx(&key);
        let (tx, rx) = oneshot::channel();
        if self.shards[idx].send(ShardCommand::Get { key, resp: tx }).await.is_ok() {
            rx.await.unwrap_or(None)
        } else {
            None
        }
    }

    pub async fn set(
        &self,
        key: Bytes,
        val: Bytes,
        ttl: Option<Duration>,
        nx: bool,
    ) -> bool {
        let idx = self.shard_idx(&key);
        let (tx, rx) = oneshot::channel();
        if self.shards[idx].send(ShardCommand::Set { key, val, ttl, nx, resp: tx }).await.is_ok() {
            rx.await.unwrap_or(false)
        } else {
            false
        }
    }

    pub async fn delete(&self, key: Bytes) -> bool {
        let idx = self.shard_idx(&key);
        let (tx, rx) = oneshot::channel();
        if self.shards[idx].send(ShardCommand::Delete { key, resp: tx }).await.is_ok() {
            rx.await.unwrap_or(false)
        } else {
            false
        }
    }

    pub async fn incr(&self, key: Bytes, delta: i64) -> Result<i64, &'static str> {
        let idx = self.shard_idx(&key);
        let (tx, rx) = oneshot::channel();
        if self.shards[idx].send(ShardCommand::Incr { key, delta, resp: tx }).await.is_ok() {
            rx.await.unwrap_or(Err("Shard error"))
        } else {
            Err("Shard send error")
        }
    }

    pub async fn ttl(&self, key: Bytes) -> i64 {
        let idx = self.shard_idx(&key);
        let (tx, rx) = oneshot::channel();
        if self.shards[idx].send(ShardCommand::Ttl { key, resp: tx }).await.is_ok() {
            rx.await.unwrap_or(-2)
        } else {
            -2
        }
    }

    pub async fn exists(&self, key: Bytes) -> bool {
        let idx = self.shard_idx(&key);
        let (tx, rx) = oneshot::channel();
        if self.shards[idx].send(ShardCommand::Exists { key, resp: tx }).await.is_ok() {
            rx.await.unwrap_or(false)
        } else {
            false
        }
    }

    pub async fn mget(&self, keys: Vec<Bytes>) -> Vec<Option<Bytes>> {
        let mut futures = Vec::with_capacity(keys.len());
        for key in keys {
            futures.push(self.get(key));
        }
        let mut results = Vec::with_capacity(futures.len());
        for fut in futures {
            results.push(fut.await);
        }
        results
    }

    pub async fn mset(&self, entries: Vec<(Bytes, Bytes)>, ttl: Option<Duration>) -> usize {
        let mut futures = Vec::with_capacity(entries.len());
        for (k, v) in entries {
            futures.push(self.set(k, v, ttl, false));
        }
        let mut count = 0;
        for fut in futures {
            if fut.await {
                count += 1;
            }
        }
        count
    }

    pub async fn stats(&self) -> ShardStats {
        let mut total = ShardStats::default();
        for shard in &self.shards {
            let (tx, rx) = oneshot::channel();
            if shard.send(ShardCommand::Stats { resp: tx }).await.is_ok() {
                if let Ok(s) = rx.await {
                    total.key_count += s.key_count;
                    total.used_memory_bytes += s.used_memory_bytes;
                    total.hits += s.hits;
                    total.misses += s.misses;
                    total.expired_keys += s.expired_keys;
                    total.evicted_keys += s.evicted_keys;
                }
            }
        }
        total
    }

    pub async fn flush(&self) {
        for shard in &self.shards {
            let (tx, rx) = oneshot::channel();
            if shard.send(ShardCommand::Flush { resp: tx }).await.is_ok() {
                let _ = rx.await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_shard_crud() {
        let mut shard = LocalShard::new(1024 * 1024);
        let k = Bytes::from("k1");
        let v = Bytes::from("v1");

        assert!(shard.set(k.clone(), v.clone(), None, false));
        assert_eq!(shard.get(&k), Some(v.clone()));
        assert!(shard.exists(&k));
        assert_eq!(shard.ttl(&k), -1);

        assert!(shard.delete(&k));
        assert_eq!(shard.get(&k), None);
        assert!(!shard.exists(&k));
        assert_eq!(shard.ttl(&k), -2);
    }

    #[test]
    fn test_setnx_and_ttl() {
        let mut shard = LocalShard::new(1024 * 1024);
        let k = Bytes::from("k2");
        let v1 = Bytes::from("first");
        let v2 = Bytes::from("second");

        assert!(shard.set(k.clone(), v1.clone(), Some(Duration::from_millis(50)), true));
        assert!(!shard.set(k.clone(), v2.clone(), None, true)); // NX fails
        assert_eq!(shard.get(&k), Some(v1));

        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(shard.get(&k), None); // Expired lazily
        assert_eq!(shard.ttl(&k), -2);
        assert!(shard.set(k.clone(), v2.clone(), None, true)); // Now succeeds
        assert_eq!(shard.get(&k), Some(v2));
    }

    #[test]
    fn test_incr_decr() {
        let mut shard = LocalShard::new(1024 * 1024);
        let k = Bytes::from("num");

        assert_eq!(shard.incr(&k, 1).unwrap(), 1);
        assert_eq!(shard.incr(&k, 5).unwrap(), 6);
        assert_eq!(shard.incr(&k, -2).unwrap(), 4);
        assert_eq!(shard.get(&k), Some(Bytes::from("4")));
    }

    #[test]
    fn test_active_cleanup() {
        let mut shard = LocalShard::new(1024 * 1024);
        let k1 = Bytes::from("exp1");
        let k2 = Bytes::from("exp2");
        let v = Bytes::from("val");

        shard.set(k1.clone(), v.clone(), Some(Duration::from_millis(20)), false);
        shard.set(k2.clone(), v.clone(), Some(Duration::from_millis(200)), false);

        std::thread::sleep(Duration::from_millis(30));
        shard.active_cleanup_job();

        assert!(!shard.store.contains_key(&k1));
        assert!(shard.store.contains_key(&k2));
    }
}
