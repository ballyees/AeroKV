use aerokv_server::memory::detect_memory_setting;
use aerokv_server::{start_server, ServerConfig};

#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let http_port = std::env::var("PORT")
        .or_else(|_| std::env::var("AEROKV_PORT"))
        .or_else(|_| std::env::var("AEROKV_HTTP_PORT"))
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(16379);

    let resp_port = std::env::var("AEROKV_RESP_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(6380);

    let token = std::env::var("AEROKV_TOKEN")
        .or_else(|_| std::env::var("AEROKV_AUTH_TOKEN"))
        .or_else(|_| std::env::var("AEROKV_PASSWORD"))
        .or_else(|_| std::env::var("AUTH_TOKEN"))
        .unwrap_or_default();
    let mem_setting = detect_memory_setting();
    let max_mem_mb = mem_setting.ceiling_mb;

    let detected_cores = num_cpus::get();
    let num_cores = std::env::var("AEROKV_CORES")
        .ok()
        .and_then(|c| c.parse::<usize>().ok())
        .unwrap_or(detected_cores)
        .max(1);

    let token_summary = if !token.is_empty() {
        let count = token.split(',').filter(|s| !s.trim().is_empty()).count();
        format!("Token-guarded ({} authorized key{})", count, if count == 1 { "" } else { "s" })
    } else {
        "Open access".to_string()
    };

    println!("⚡ Initializing AeroKV Server");
    println!("   - Hardware cores detected: {} (Active shards: {})", detected_cores, num_cores);
    println!("   - Memory ceiling: {} MB ({})", max_mem_mb, mem_setting.description());
    println!("   - HTTP/1.1 & HTTP/2 TCP port: {}", http_port);
    println!("   - High-throughput RESP TCP port: {}", resp_port);
    println!("   - Security: {}", token_summary);

    let config = ServerConfig {
        http_port,
        resp_port,
        token,
        max_mem_mb,
        num_cores,
    };

    let handle = start_server(config).await?;
    println!("🚀 HTTP/1.1 & HTTP/2 listener active on {}", handle.http_addr);
    println!("🚀 High-speed RESP listener active on {}", handle.resp_addr);

    tokio::signal::ctrl_c().await?;
    println!("\n🛑 Graceful shutdown initiated.");
    let _ = handle.shutdown_tx.send(());
    Ok(())
}
