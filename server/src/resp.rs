use crate::auth::Authenticator;
use crate::shard::{BatchOp, BatchResult, ShardPool};
use bytes::{Bytes, BytesMut};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub async fn run_resp_server(
    listener: TcpListener,
    auth: Authenticator,
    shard_pool: ShardPool,
) {
    while let Ok((stream, _)) = listener.accept().await {
        let _ = stream.set_nodelay(true);
        let auth = auth.clone();
        let shard_pool = shard_pool.clone();
        tokio::spawn(async move {
            handle_resp_connection(stream, auth, shard_pool).await;
        });
    }
}

async fn handle_resp_connection(
    mut stream: TcpStream,
    auth: Authenticator,
    shard_pool: ShardPool,
) {
    // Start with minimal 4 KB buffers so 100k idle connections consume only ~800 MB instead of 12.8 GB
    let mut read_buf = BytesMut::with_capacity(4096);
    let mut write_buf = BytesMut::with_capacity(4096);
    let mut authenticated = !auth.is_enabled();

    loop {
        // Expand read_buf dynamically only when approaching full capacity for large pipelines
        if read_buf.capacity() - read_buf.len() < 1024 {
            read_buf.reserve(8192);
        }

        let n = match stream.read_buf(&mut read_buf).await {
            Ok(0) => break, // EOF
            Ok(n) => n,
            Err(_) => break,
        };

        let _ = n;

        // Parse all available commands from read_buf
        let mut commands = Vec::new();
        while let Some(cmd) = parse_resp_command(&mut read_buf) {
            commands.push(cmd);
        }

        if commands.is_empty() {
            continue;
        }

        write_buf.clear();

        // Process commands (pipelined)
        if commands.len() == 1 {
            let cmd = commands.pop().unwrap();
            process_single_command(cmd, &auth, &mut authenticated, &shard_pool, &mut write_buf).await;
        } else {
            // Check if there are non-shard or auth commands in pipeline
            let mut can_batch = true;
            for c in &commands {
                if is_special_command(&c.name) {
                    can_batch = false;
                    break;
                }
            }

            if !authenticated || !can_batch {
                for cmd in commands {
                    process_single_command(cmd, &auth, &mut authenticated, &shard_pool, &mut write_buf).await;
                }
            } else {
                process_pipelined_batch(commands, &shard_pool, &mut write_buf).await;
            }
        }

        if !write_buf.is_empty() && stream.write_all(&write_buf).await.is_err() {
            break;
        }

        // Adaptive shrink: If buffers ballooned due to a massive batch/value, shrink back to conserve RAM for C100k
        if read_buf.capacity() > 64 * 1024 && read_buf.is_empty() {
            read_buf = BytesMut::with_capacity(4096);
        }
        if write_buf.capacity() > 64 * 1024 {
            write_buf = BytesMut::with_capacity(4096);
        }
    }
}

pub struct ParsedCommand {
    pub name: String,
    pub args: Vec<Bytes>,
}

fn parse_resp_command(buf: &mut BytesMut) -> Option<ParsedCommand> {
    if buf.is_empty() {
        return None;
    }

    if buf[0] == b'*' {
        // Multi-bulk array: *<num>\r\n$<len>\r\n<arg>\r\n...
        let line_end = find_crlf(buf, 1)?;
        let num_str = std::str::from_utf8(&buf[1..line_end]).ok()?;
        let num_args: usize = num_str.parse().ok()?;

        let mut offset = line_end + 2;
        let mut args = Vec::with_capacity(num_args);

        for _ in 0..num_args {
            if offset >= buf.len() || buf[offset] != b'$' {
                return None;
            }
            let len_end = find_crlf(buf, offset + 1)?;
            let len_str = std::str::from_utf8(&buf[offset + 1..len_end]).ok()?;
            let arg_len: usize = len_str.parse().ok()?;

            let arg_start = len_end + 2;
            let arg_end = arg_start + arg_len;
            if arg_end + 2 > buf.len() {
                return None;
            }
            if &buf[arg_end..arg_end + 2] != b"\r\n" {
                return None;
            }

            args.push(Bytes::copy_from_slice(&buf[arg_start..arg_end]));
            offset = arg_end + 2;
        }

        let _ = buf.split_to(offset);

        if args.is_empty() {
            return None;
        }

        let name = match args[0].as_ref() {
            b"GET" | b"get" => "GET".to_string(),
            b"SET" | b"set" => "SET".to_string(),
            b"INCR" | b"incr" => "INCR".to_string(),
            b"INCRBY" | b"incrby" => "INCRBY".to_string(),
            b"DEL" | b"del" => "DEL".to_string(),
            b"PING" | b"ping" => "PING".to_string(),
            _ => std::str::from_utf8(&args[0])
                .unwrap_or("")
                .to_uppercase(),
        };
        let cmd_args = args.into_iter().skip(1).collect();

        Some(ParsedCommand {
            name,
            args: cmd_args,
        })
    } else {
        // Inline command e.g. "PING\r\n"
        let line_end = find_crlf(buf, 0)?;
        let line = std::str::from_utf8(&buf[..line_end]).ok()?;
        let mut parts = line.split_whitespace();
        let name = parts.next()?.to_uppercase();
        let args = parts.map(|s| Bytes::copy_from_slice(s.as_bytes())).collect();

        let _ = buf.split_to(line_end + 2);
        Some(ParsedCommand { name, args })
    }
}

fn find_crlf(buf: &[u8], start: usize) -> Option<usize> {
    if start >= buf.len() {
        return None;
    }
    (start..buf.len().saturating_sub(1)).find(|&i| buf[i] == b'\r' && buf[i + 1] == b'\n')
}

fn is_special_command(name: &str) -> bool {
    matches!(
        name,
        "AUTH" | "PING" | "COMMAND" | "CONFIG" | "INFO" | "STATS" | "FLUSHDB" | "FLUSHALL" | "HELLO" | "QUIT"
    )
}

async fn process_single_command(
    cmd: ParsedCommand,
    auth: &Authenticator,
    authenticated: &mut bool,
    shard_pool: &ShardPool,
    out: &mut BytesMut,
) {
    let name = cmd.name.as_str();

    // Handle authentication
    if name == "AUTH" {
        if cmd.args.is_empty() {
            out.extend_from_slice(b"-ERR wrong number of arguments for 'auth' command\r\n");
            return;
        }
        let token = &cmd.args[0];
        if auth.verify(token) {
            *authenticated = true;
            out.extend_from_slice(b"+OK\r\n");
        } else {
            out.extend_from_slice(b"-ERR invalid password\r\n");
        }
        return;
    }

    if !*authenticated {
        out.extend_from_slice(b"-NOAUTH Authentication required.\r\n");
        return;
    }

    match name {
        "PING" => {
            if let Some(arg) = cmd.args.first() {
                append_bulk_string(out, arg);
            } else {
                out.extend_from_slice(b"+PONG\r\n");
            }
        }

        "COMMAND" => {
            // For redis-cli / redis-benchmark handshake
            out.extend_from_slice(b"*0\r\n");
        }

        "HELLO" => {
            // For redis-cli / redis-benchmark handshake
            out.extend_from_slice(b"%1\r\n+proto\r\n:2\r\n");
        }

        "CONFIG" => {
            out.extend_from_slice(b"*0\r\n");
        }

        "GET" => {
            if cmd.args.is_empty() {
                out.extend_from_slice(b"-ERR wrong number of arguments for 'get' command\r\n");
                return;
            }
            let key = cmd.args[0].clone();
            match shard_pool.get(key).await {
                Some(val) => append_bulk_string(out, &val),
                None => out.extend_from_slice(b"$-1\r\n"),
            }
        }

        "SET" => {
            if cmd.args.len() < 2 {
                out.extend_from_slice(b"-ERR wrong number of arguments for 'set' command\r\n");
                return;
            }
            let key = cmd.args[0].clone();
            let val = cmd.args[1].clone();
            let mut ttl = None;
            let mut nx = false;

            let mut idx = 2;
            while idx < cmd.args.len() {
                let opt = std::str::from_utf8(&cmd.args[idx]).unwrap_or("").to_uppercase();
                if opt == "NX" {
                    nx = true;
                    idx += 1;
                } else if opt == "EX" && idx + 1 < cmd.args.len() {
                    if let Ok(s) = std::str::from_utf8(&cmd.args[idx + 1]) {
                        if let Ok(secs) = s.parse::<u64>() {
                            ttl = Some(Duration::from_secs(secs));
                        }
                    }
                    idx += 2;
                } else if opt == "PX" && idx + 1 < cmd.args.len() {
                    if let Ok(s) = std::str::from_utf8(&cmd.args[idx + 1]) {
                        if let Ok(ms) = s.parse::<u64>() {
                            ttl = Some(Duration::from_millis(ms));
                        }
                    }
                    idx += 2;
                } else {
                    idx += 1;
                }
            }

            let ok = shard_pool.set(key, val, ttl, nx).await;
            if ok {
                out.extend_from_slice(b"+OK\r\n");
            } else if nx {
                out.extend_from_slice(b"$-1\r\n");
            } else {
                out.extend_from_slice(b"+OK\r\n");
            }
        }

        "SETNX" => {
            if cmd.args.len() < 2 {
                out.extend_from_slice(b"-ERR wrong number of arguments for 'setnx' command\r\n");
                return;
            }
            let key = cmd.args[0].clone();
            let val = cmd.args[1].clone();
            let ok = shard_pool.set(key, val, None, true).await;
            if ok {
                out.extend_from_slice(b":1\r\n");
            } else {
                out.extend_from_slice(b":0\r\n");
            }
        }

        "EXP" => {
            if cmd.args.len() < 3 {
                out.extend_from_slice(b"-ERR wrong number of arguments for 'exp' command\r\n");
                return;
            }
            let key = cmd.args[0].clone();
            let ms: u64 = std::str::from_utf8(&cmd.args[1])
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let val = cmd.args[2].clone();
            let ok = shard_pool.set(key, val, Some(Duration::from_millis(ms)), false).await;
            if ok {
                out.extend_from_slice(b"+OK\r\n");
            } else {
                out.extend_from_slice(b"-ERR set failed\r\n");
            }
        }

        "DEL" => {
            if cmd.args.is_empty() {
                out.extend_from_slice(b"-ERR wrong number of arguments for 'del' command\r\n");
                return;
            }
            let mut count = 0;
            for k in cmd.args {
                if shard_pool.delete(k).await {
                    count += 1;
                }
            }
            append_integer(out, count);
        }

        "INCR" => {
            if cmd.args.is_empty() {
                out.extend_from_slice(b"-ERR wrong number of arguments for 'incr' command\r\n");
                return;
            }
            let key = cmd.args[0].clone();
            match shard_pool.incr(key, 1).await {
                Ok(n) => append_integer(out, n),
                Err(e) => {
                    out.extend_from_slice(b"-");
                    out.extend_from_slice(e.as_bytes());
                    out.extend_from_slice(b"\r\n");
                }
            }
        }

        "DECR" => {
            if cmd.args.is_empty() {
                out.extend_from_slice(b"-ERR wrong number of arguments for 'decr' command\r\n");
                return;
            }
            let key = cmd.args[0].clone();
            match shard_pool.incr(key, -1).await {
                Ok(n) => append_integer(out, n),
                Err(e) => {
                    out.extend_from_slice(b"-");
                    out.extend_from_slice(e.as_bytes());
                    out.extend_from_slice(b"\r\n");
                }
            }
        }

        "INCRBY" => {
            if cmd.args.len() < 2 {
                out.extend_from_slice(b"-ERR wrong number of arguments for 'incrby' command\r\n");
                return;
            }
            let key = cmd.args[0].clone();
            let delta: i64 = std::str::from_utf8(&cmd.args[1])
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            match shard_pool.incr(key, delta).await {
                Ok(n) => append_integer(out, n),
                Err(e) => {
                    out.extend_from_slice(b"-");
                    out.extend_from_slice(e.as_bytes());
                    out.extend_from_slice(b"\r\n");
                }
            }
        }

        "DECRBY" => {
            if cmd.args.len() < 2 {
                out.extend_from_slice(b"-ERR wrong number of arguments for 'decrby' command\r\n");
                return;
            }
            let key = cmd.args[0].clone();
            let delta: i64 = std::str::from_utf8(&cmd.args[1])
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            match shard_pool.incr(key, -delta).await {
                Ok(n) => append_integer(out, n),
                Err(e) => {
                    out.extend_from_slice(b"-");
                    out.extend_from_slice(e.as_bytes());
                    out.extend_from_slice(b"\r\n");
                }
            }
        }

        "TTL" => {
            if cmd.args.is_empty() {
                out.extend_from_slice(b"-ERR wrong number of arguments for 'ttl' command\r\n");
                return;
            }
            let key = cmd.args[0].clone();
            let ms = shard_pool.ttl(key).await;
            let secs = if ms > 0 { ms / 1000 } else { ms };
            append_integer(out, secs);
        }

        "PTTL" => {
            if cmd.args.is_empty() {
                out.extend_from_slice(b"-ERR wrong number of arguments for 'pttl' command\r\n");
                return;
            }
            let key = cmd.args[0].clone();
            let ms = shard_pool.ttl(key).await;
            append_integer(out, ms);
        }

        "EXISTS" => {
            if cmd.args.is_empty() {
                out.extend_from_slice(b"-ERR wrong number of arguments for 'exists' command\r\n");
                return;
            }
            let mut count = 0;
            for k in cmd.args {
                if shard_pool.exists(k).await {
                    count += 1;
                }
            }
            append_integer(out, count);
        }

        "MGET" => {
            let vals = shard_pool.mget(cmd.args).await;
            out.extend_from_slice(b"*");
            out.extend_from_slice(vals.len().to_string().as_bytes());
            out.extend_from_slice(b"\r\n");
            for v in vals {
                match v {
                    Some(b) => append_bulk_string(out, &b),
                    None => out.extend_from_slice(b"$-1\r\n"),
                }
            }
        }

        "MSET" => {
            if !cmd.args.len().is_multiple_of(2) {
                out.extend_from_slice(b"-ERR wrong number of arguments for 'mset' command\r\n");
                return;
            }
            let mut pairs = Vec::with_capacity(cmd.args.len() / 2);
            let mut i = 0;
            while i < cmd.args.len() {
                pairs.push((cmd.args[i].clone(), cmd.args[i + 1].clone()));
                i += 2;
            }
            let _ = shard_pool.mset(pairs, None).await;
            out.extend_from_slice(b"+OK\r\n");
        }

        "FLUSHDB" | "FLUSHALL" => {
            shard_pool.flush().await;
            out.extend_from_slice(b"+OK\r\n");
        }

        "INFO" | "STATS" => {
            let stats = shard_pool.stats().await;
            let info_str = format!(
                "# Keyspace\r\ndb0:keys={},expires={}\r\n# Memory\r\nused_memory:{}\r\n# Stats\r\ntotal_hits:{}\r\ntotal_misses:{}\r\nevicted_keys:{}\r\n",
                stats.key_count, stats.expired_keys, stats.used_memory_bytes, stats.hits, stats.misses, stats.evicted_keys
            );
            append_bulk_string(out, info_str.as_bytes());
        }

        "QUIT" => {
            out.extend_from_slice(b"+OK\r\n");
        }

        _ => {
            out.extend_from_slice(b"-ERR unknown command '");
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(b"'\r\n");
        }
    }
}

async fn process_pipelined_batch(
    commands: Vec<ParsedCommand>,
    shard_pool: &ShardPool,
    out: &mut BytesMut,
) {
    let num_shards = shard_pool.shard_count();
    let mut shard_batches: Vec<Vec<BatchOp>> = (0..num_shards).map(|_| Vec::new()).collect();
    let mut routing_plan: Vec<(usize, usize)> = Vec::with_capacity(commands.len());

    for cmd in commands {
        let name = cmd.name.as_str();
        match name {
            "GET" if !cmd.args.is_empty() => {
                let key = cmd.args[0].clone();
                let s_idx = shard_pool.shard_idx(&key);
                let pos = shard_batches[s_idx].len();
                shard_batches[s_idx].push(BatchOp::Get(key));
                routing_plan.push((s_idx, pos));
            }
            "SET" if cmd.args.len() >= 2 => {
                let key = cmd.args[0].clone();
                let val = cmd.args[1].clone();
                let s_idx = shard_pool.shard_idx(&key);
                let pos = shard_batches[s_idx].len();
                shard_batches[s_idx].push(BatchOp::Set {
                    key,
                    val,
                    ttl: None,
                    nx: false,
                });
                routing_plan.push((s_idx, pos));
            }
            "INCR" if !cmd.args.is_empty() => {
                let key = cmd.args[0].clone();
                let s_idx = shard_pool.shard_idx(&key);
                let pos = shard_batches[s_idx].len();
                shard_batches[s_idx].push(BatchOp::Incr { key, delta: 1 });
                routing_plan.push((s_idx, pos));
            }
            "INCRBY" if cmd.args.len() >= 2 => {
                let key = cmd.args[0].clone();
                let delta: i64 = std::str::from_utf8(&cmd.args[1])
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                let s_idx = shard_pool.shard_idx(&key);
                let pos = shard_batches[s_idx].len();
                shard_batches[s_idx].push(BatchOp::Incr { key, delta });
                routing_plan.push((s_idx, pos));
            }
            "DEL" if !cmd.args.is_empty() => {
                let key = cmd.args[0].clone();
                let s_idx = shard_pool.shard_idx(&key);
                let pos = shard_batches[s_idx].len();
                shard_batches[s_idx].push(BatchOp::Delete(key));
                routing_plan.push((s_idx, pos));
            }
            _ => {
                // Fallback: not batchable in fastpath
                routing_plan.push((usize::MAX, 0));
            }
        }
    }

    // Execute batches concurrently across all touched shards
    let mut shard_futures = Vec::with_capacity(num_shards);
    for (s_idx, batch) in shard_batches.into_iter().enumerate() {
        if batch.is_empty() {
            shard_futures.push(None);
        } else {
            shard_futures.push(Some(shard_pool.execute_batch(s_idx, batch)));
        }
    }

    let mut shard_results = Vec::with_capacity(num_shards);
    for opt_fut in shard_futures {
        if let Some(fut) = opt_fut {
            shard_results.push(fut.await);
        } else {
            shard_results.push(Vec::new());
        }
    }

    // Reassemble results in original command sequence into write_buf
    for (s_idx, pos) in routing_plan {
        if s_idx == usize::MAX {
            out.extend_from_slice(b"-ERR unsupported batch operation\r\n");
            continue;
        }

        if let Some(res) = shard_results[s_idx].get(pos) {
            match res {
                BatchResult::Value(Some(v)) => append_bulk_string(out, v),
                BatchResult::Value(None) => out.extend_from_slice(b"$-1\r\n"),
                BatchResult::Bool(true) => out.extend_from_slice(b"+OK\r\n"),
                BatchResult::Bool(false) => out.extend_from_slice(b"$-1\r\n"),
                BatchResult::Int(n) => append_integer(out, *n),
                BatchResult::Error(e) => {
                    out.extend_from_slice(b"-");
                    out.extend_from_slice(e.as_bytes());
                    out.extend_from_slice(b"\r\n");
                }
            }
        } else {
            out.extend_from_slice(b"-ERR internal shard error\r\n");
        }
    }
}

#[inline]
fn append_bulk_string(out: &mut BytesMut, s: &[u8]) {
    out.extend_from_slice(b"$");
    out.extend_from_slice(s.len().to_string().as_bytes());
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(s);
    out.extend_from_slice(b"\r\n");
}

#[inline]
fn append_integer<T: itoa::Integer>(out: &mut BytesMut, n: T) {
    let mut itoa_buf = itoa::Buffer::new();
    out.extend_from_slice(b":");
    out.extend_from_slice(itoa_buf.format(n).as_bytes());
    out.extend_from_slice(b"\r\n");
}
