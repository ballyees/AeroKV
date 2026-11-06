use bytes::Bytes;
use clap::Parser;
use core_client::AeroClient;
use hdrhistogram::Histogram;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(name = "aerokv-bench", about = "Performance benchmark suite for AeroKV")]
struct Args {
    #[arg(short = 'H', long, default_value = "127.0.0.1")]
    host: String,

    #[arg(long, default_value_t = 16379)]
    aerokv_http_port: u16,

    #[arg(long, default_value_t = 6380)]
    aerokv_resp_port: u16,

    #[arg(long, default_value_t = 6379)]
    redis_port: u16,

    #[arg(long, default_value_t = 6381)]
    dragonfly_port: u16,

    #[arg(short = 'c', long, default_value_t = 50)]
    concurrency: usize,

    #[arg(short = 'n', long, alias = "total", default_value_t = 50_000)]
    requests: usize,

    #[arg(long, default_value = "all")]
    protocol: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let run_resp = args.protocol.eq_ignore_ascii_case("all") || args.protocol.eq_ignore_ascii_case("resp");
    let run_http = args.protocol.eq_ignore_ascii_case("all") || args.protocol.eq_ignore_ascii_case("http");

    println!("================================================================");
    println!("🚀 AeroKV Benchmark Matrix (RESP vs HTTP/1.1 vs HTTP/2 vs Redis vs Dragonfly)");
    println!("================================================================");
    println!("Host:              {}", args.host);
    println!("AeroKV RESP port:  {}", args.aerokv_resp_port);
    println!("AeroKV HTTP port:  {}", args.aerokv_http_port);
    println!("Redis port:        {}", args.redis_port);
    println!("Dragonfly port:    {}", args.dragonfly_port);
    println!("Protocol filter:   {}", args.protocol);
    println!("Concurrency:       {} workers", args.concurrency);
    println!("Requests/Phase:    {}", args.requests);
    println!("----------------------------------------------------------------");

    if run_resp {
        // 1. Benchmark AeroKV via RESP (Single non-pipelined)
        println!("\n▶ [1/4] Benchmarking AeroKV via RESP (Pipelining: 1)...");
        let aerokv_resp_url = format!("redis://{}:{}", args.host, args.aerokv_resp_port);
        match run_resp_bench(&aerokv_resp_url, args.concurrency, args.requests, 1).await {
            Ok((set_h, get_h, incr_h)) => {
                print_metrics("AeroKV RESP SET", &set_h);
                print_metrics("AeroKV RESP GET", &get_h);
                print_metrics("AeroKV RESP INCR", &incr_h);
            }
            Err(e) => eprintln!("❌ AeroKV RESP benchmark failed: {}", e),
        }

        // 2. Benchmark AeroKV via RESP (Pipelining: 16)
        println!("\n▶ [2/4] Benchmarking AeroKV via RESP (Pipelining: 16)...");
        match run_resp_bench(&aerokv_resp_url, args.concurrency, args.requests, 16).await {
            Ok((set_h, get_h, incr_h)) => {
                print_metrics("AeroKV RESP-P16 SET", &set_h);
                print_metrics("AeroKV RESP-P16 GET", &get_h);
                print_metrics("AeroKV RESP-P16 INCR", &incr_h);
            }
            Err(e) => eprintln!("❌ AeroKV RESP Pipelined benchmark failed: {}", e),
        }

        // 3. Benchmark Redis (Reference on redis_port)
        println!("\n▶ [3/4] Benchmarking Redis Reference on port {} (Pipelining: 1)...", args.redis_port);
        let redis_url = format!("redis://{}:{}", args.host, args.redis_port);
        match run_resp_bench(&redis_url, args.concurrency, args.requests, 1).await {
            Ok((set_h, get_h, incr_h)) => {
                print_metrics("Redis SET", &set_h);
                print_metrics("Redis GET", &get_h);
                print_metrics("Redis INCR", &incr_h);
            }
            Err(e) => eprintln!("ℹ Notice: Redis benchmark skipped or unreachable on port {}: {}", args.redis_port, e),
        }

        // 4. Benchmark Dragonfly (Reference on dragonfly_port)
        println!("\n▶ [4/4] Benchmarking Dragonfly Reference on port {} (Pipelining: 1)...", args.dragonfly_port);
        let dragonfly_url = format!("redis://{}:{}", args.host, args.dragonfly_port);
        match run_resp_bench(&dragonfly_url, args.concurrency, args.requests, 1).await {
            Ok((set_h, get_h, incr_h)) => {
                print_metrics("Dragonfly SET", &set_h);
                print_metrics("Dragonfly GET", &get_h);
                print_metrics("Dragonfly INCR", &incr_h);
            }
            Err(e) => eprintln!("ℹ Notice: Dragonfly benchmark skipped or unreachable on port {}: {}", args.dragonfly_port, e),
        }
    }

    // 5. Benchmark AeroKV via HTTP/1.1 & HTTP/2
    if run_http {
        println!("\n▶ Benchmarking AeroKV via HTTP/1.1 (TCP Keep-Alive)...");
        match AeroClient::connect(&args.host, args.aerokv_http_port, None).await {
            Ok(client) => {
                let _ = client.ping().await;
                let set_h = run_http_bench(Arc::clone(&client), args.concurrency, args.requests, |c, idx| {
                    let k = format!("bench:h1:{}", idx);
                    let v = Bytes::from("val_payload_128_bytes_padding_0123456789abcdef0123456789abcdef");
                    async move { c.set(&k, v, None).await.map(|_| ()) }
                }).await;

                let get_h = run_http_bench(Arc::clone(&client), args.concurrency, args.requests, |c, idx| {
                    let k = format!("bench:h1:{}", idx);
                    async move { c.get(&k).await.map(|_| ()) }
                }).await;

                print_metrics("AeroKV H1 SET", &set_h);
                print_metrics("AeroKV H1 GET", &get_h);
            }
            Err(e) => eprintln!("❌ AeroKV HTTP client connect failed: {}", e),
        }

        println!("\n▶ Benchmarking AeroKV via HTTP/2 (Cleartext h2c)...");
        let payload = Bytes::from("val_payload_128_bytes_padding_0123456789abcdef0123456789abcdef");
        match run_h2_bench(&args.host, args.aerokv_http_port, args.concurrency, args.requests, http::Method::PUT, "/kv/bench_h2_key", Some(payload)).await {
            Ok(set_h) => print_metrics("AeroKV H2 SET", &set_h),
            Err(e) => eprintln!("❌ AeroKV H2 SET failed: {}", e),
        }
        match run_h2_bench(&args.host, args.aerokv_http_port, args.concurrency, args.requests, http::Method::GET, "/kv/bench_h2_key", None).await {
            Ok(get_h) => print_metrics("AeroKV H2 GET", &get_h),
            Err(e) => eprintln!("❌ AeroKV H2 GET failed: {}", e),
        }
    }

    println!("\n✅ Benchmark matrix complete.");
    Ok(())
}

async fn run_resp_bench(
    url: &str,
    concurrency: usize,
    total_ops: usize,
    pipeline_depth: usize,
) -> Result<(Histogram<u64>, Histogram<u64>, Histogram<u64>), Box<dyn std::error::Error + Send + Sync>> {
    let client = redis::Client::open(url)?;
    // Test connectivity
    let mut probe_con = client.get_multiplexed_async_connection().await?;
    let probe_pong: String = redis::cmd("PING").query_async(&mut probe_con).await?;
    if probe_pong != "PONG" {
        return Err(format!("Unexpected ping response: {}", probe_pong).into());
    }

    // SET phase
    let set_h = run_resp_phase(client.clone(), concurrency, total_ops, pipeline_depth, |pipe, i| {
        pipe.set(format!("k:{}", i), "val_payload_128_bytes_padding_0123456789abcdef0123456789abcdef");
    }).await?;

    // GET phase
    let get_h = run_resp_phase(client.clone(), concurrency, total_ops, pipeline_depth, |pipe, i| {
        pipe.get(format!("k:{}", i));
    }).await?;

    // INCR phase
    let incr_h = run_resp_phase(client.clone(), concurrency, total_ops, pipeline_depth, |pipe, i| {
        pipe.incr(format!("counter:{}", i % 1000), 1);
    }).await?;

    Ok((set_h, get_h, incr_h))
}

async fn run_resp_phase<F>(
    client: redis::Client,
    concurrency: usize,
    total_ops: usize,
    pipeline_depth: usize,
    mut build_cmd: F,
) -> Result<Histogram<u64>, Box<dyn std::error::Error + Send + Sync>>
where
    F: FnMut(&mut redis::Pipeline, usize) + Send + Sync + 'static + Copy,
{
    let counter = Arc::new(AtomicUsize::new(0));
    let errors = Arc::new(AtomicUsize::new(0));
    let start = Instant::now();
    let mut handles = Vec::with_capacity(concurrency);

    for _ in 0..concurrency {
        let client = client.clone();
        let counter = Arc::clone(&counter);
        let errors = Arc::clone(&errors);

        let handle = tokio::spawn(async move {
            let mut hist = Histogram::<u64>::new_with_bounds(1, 10_000_000, 3).unwrap();
            let mut con = match client.get_multiplexed_async_connection().await {
                Ok(c) => c,
                Err(_) => {
                    errors.fetch_add(1, Ordering::Relaxed);
                    return hist;
                }
            };

            loop {
                let start_idx = counter.fetch_add(pipeline_depth, Ordering::Relaxed);
                if start_idx >= total_ops {
                    break;
                }

                let batch_size = pipeline_depth.min(total_ops - start_idx);
                let mut pipe = redis::pipe();
                for offset in 0..batch_size {
                    build_cmd(&mut pipe, start_idx + offset);
                }

                let t0 = Instant::now();
                let res: Result<(), _> = pipe.query_async(&mut con).await;
                let elapsed_us = t0.elapsed().as_micros() as u64;

                match res {
                    Ok(_) => {
                        let per_op_us = (elapsed_us / (batch_size as u64)).max(1);
                        for _ in 0..batch_size {
                            let _ = hist.record(per_op_us);
                        }
                    }
                    Err(_) => {
                        errors.fetch_add(batch_size, Ordering::Relaxed);
                    }
                }
            }
            hist
        });
        handles.push(handle);
    }

    let mut combined_hist = Histogram::<u64>::new_with_bounds(1, 10_000_000, 3).unwrap();
    for handle in handles {
        if let Ok(hist) = handle.await {
            let _ = combined_hist.add(hist);
        }
    }

    let total_errs = errors.load(Ordering::Relaxed);
    let duration = start.elapsed();
    let successful_ops = total_ops.saturating_sub(total_errs);
    let ops_sec = (successful_ops as f64) / duration.as_secs_f64();
    println!("   Completed {} ops in {:.2?} (~{:.0} ops/sec, errors: {})", successful_ops, duration, ops_sec, total_errs);

    if total_errs > 0 && total_errs >= total_ops / 2 {
        return Err(format!("Phase encountered excessive errors ({} failed)", total_errs).into());
    }

    Ok(combined_hist)
}

async fn run_http_bench<F, Fut>(
    client: Arc<AeroClient>,
    concurrency: usize,
    total_ops: usize,
    op: F,
) -> Histogram<u64>
where
    F: Fn(Arc<AeroClient>, usize) -> Fut + Send + Sync + 'static + Copy,
    Fut: std::future::Future<Output = Result<(), core_client::AeroError>> + Send,
{
    let counter = Arc::new(AtomicUsize::new(0));
    let errors = Arc::new(AtomicUsize::new(0));
    let start = Instant::now();
    let mut handles = Vec::with_capacity(concurrency);

    for _ in 0..concurrency {
        let client = Arc::clone(&client);
        let counter = Arc::clone(&counter);
        let errors = Arc::clone(&errors);

        let handle = tokio::spawn(async move {
            let mut hist = Histogram::<u64>::new_with_bounds(1, 10_000_000, 3).unwrap();
            loop {
                let idx = counter.fetch_add(1, Ordering::Relaxed);
                if idx >= total_ops {
                    break;
                }

                let t0 = Instant::now();
                match op(Arc::clone(&client), idx).await {
                    Ok(_) => {
                        let elapsed_us = t0.elapsed().as_micros() as u64;
                        let _ = hist.record(elapsed_us.max(1));
                    }
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            hist
        });
        handles.push(handle);
    }

    let mut combined_hist = Histogram::<u64>::new_with_bounds(1, 10_000_000, 3).unwrap();
    for handle in handles {
        if let Ok(hist) = handle.await {
            let _ = combined_hist.add(hist);
        }
    }

    let total_errs = errors.load(Ordering::Relaxed);
    let duration = start.elapsed();
    let successful_ops = total_ops.saturating_sub(total_errs);
    let ops_sec = (successful_ops as f64) / duration.as_secs_f64();
    println!("   Completed {} ops in {:.2?} (~{:.0} ops/sec, errors: {})", successful_ops, duration, ops_sec, total_errs);

    combined_hist
}

fn print_metrics(label: &str, hist: &Histogram<u64>) {
    println!(
        "{:<20} | p50: {:>5.2} ms | p95: {:>5.2} ms | p99: {:>5.2} ms | max: {:>5.2} ms",
        label,
        hist.value_at_quantile(0.50) as f64 / 1000.0,
        hist.value_at_quantile(0.95) as f64 / 1000.0,
        hist.value_at_quantile(0.99) as f64 / 1000.0,
        hist.max() as f64 / 1000.0,
    );
}

async fn run_h2_bench(
    host: &str,
    port: u16,
    concurrency: usize,
    total_ops: usize,
    method: http::Method,
    path: &str,
    body_data: Option<Bytes>,
) -> Result<Histogram<u64>, Box<dyn std::error::Error + Send + Sync>> {
    let counter = Arc::new(AtomicUsize::new(0));
    let errors = Arc::new(AtomicUsize::new(0));
    let start = Instant::now();
    let mut handles = Vec::with_capacity(concurrency);
    let addr = format!("{}:{}", host, port);

    for _ in 0..concurrency {
        let counter = Arc::clone(&counter);
        let errors = Arc::clone(&errors);
        let addr = addr.clone();
        let path = path.to_string();
        let method = method.clone();
        let body_data = body_data.clone();

        let handle = tokio::spawn(async move {
            let mut hist = Histogram::<u64>::new_with_bounds(1, 10_000_000, 3).unwrap();
            let stream = match tokio::net::TcpStream::connect(&addr).await {
                Ok(s) => {
                    let _ = s.set_nodelay(true);
                    s
                }
                Err(_) => {
                    errors.fetch_add(total_ops / concurrency, Ordering::Relaxed);
                    return hist;
                }
            };

            let (mut client, h2_conn) = match h2::client::handshake(stream).await {
                Ok(res) => res,
                Err(_) => {
                    errors.fetch_add(total_ops / concurrency, Ordering::Relaxed);
                    return hist;
                }
            };

            tokio::spawn(async move {
                let _ = h2_conn.await;
            });

            loop {
                let idx = counter.fetch_add(1, Ordering::Relaxed);
                if idx >= total_ops {
                    break;
                }

                let t0 = Instant::now();
                client = match client.ready().await {
                    Ok(c) => c,
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                        break;
                    }
                };

                let end_stream = body_data.is_none();
                let uri_str = format!("http://{}{}", addr, path);
                let uri: http::Uri = match uri_str.parse() {
                    Ok(u) => u,
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                };

                let req = http::Request::builder()
                    .method(method.clone())
                    .uri(uri)
                    .body(())
                    .unwrap();

                let (response_future, mut send_stream) = match client.send_request(req, end_stream) {
                    Ok(pair) => pair,
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                };

                if let Some(ref data) = body_data {
                    if send_stream.send_data(data.clone(), true).is_err() {
                        errors.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                }

                match response_future.await {
                    Ok(resp) => {
                        let mut body = resp.into_body();
                        let mut ok = true;
                        while let Some(chunk_res) = body.data().await {
                            match chunk_res {
                                Ok(chunk) => {
                                    let _ = body.flow_control().release_capacity(chunk.len());
                                }
                                Err(_) => {
                                    ok = false;
                                    break;
                                }
                            }
                        }
                        if ok {
                            let elapsed_us = t0.elapsed().as_micros() as u64;
                            let _ = hist.record(elapsed_us.max(1));
                        } else {
                            errors.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            hist
        });
        handles.push(handle);
    }

    let mut combined_hist = Histogram::<u64>::new_with_bounds(1, 10_000_000, 3).unwrap();
    for handle in handles {
        if let Ok(hist) = handle.await {
            let _ = combined_hist.add(hist);
        }
    }

    let total_errs = errors.load(Ordering::Relaxed);
    let duration = start.elapsed();
    let successful_ops = total_ops.saturating_sub(total_errs);
    let ops_sec = (successful_ops as f64) / duration.as_secs_f64();
    println!("   Completed {} ops in {:.2?} (~{:.0} ops/sec, errors: {})", successful_ops, duration, ops_sec, total_errs);

    Ok(combined_hist)
}
