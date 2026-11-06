use bytes::Bytes;
use clap::Parser;
use core_client::AeroClient;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "aerokv-cli", about = "Interactive CLI & REPL for AeroKV")]
struct Args {
    #[arg(short = 'H', long, default_value = "127.0.0.1")]
    host: String,

    #[arg(short, long, default_value_t = 6379)]
    port: u16,

    #[arg(short, long)]
    token: Option<String>,

    #[arg(short = 'P', long)]
    prefix: Option<String>,

    #[arg(trailing_var_arg = true)]
    command: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let client = AeroClient::connect_with_prefix(
        &args.host,
        args.port,
        args.token.as_deref(),
        args.prefix.as_deref(),
    )
    .await?;

    if !args.command.is_empty() {
        let line = args.command.join(" ");
        execute_command(&client, &line).await;
        return Ok(());
    }

    let prefix_info = args
        .prefix
        .as_deref()
        .map(|p| format!(" | Prefix: {}", p))
        .unwrap_or_default();
    println!("⚡ Welcome to AeroKV Interactive Shell");
    println!("   Connecting to {}:{} | Protocol: {}{}", args.host, args.port, client.active_protocol(), prefix_info);
    println!("   Type HELP for command list or EXIT to leave.\n");

    let mut rl = DefaultEditor::new()?;

    loop {
        let proto = client.active_protocol();
        let prompt = format!("aerokv[{}]> ", proto);
        match rl.readline(&prompt) {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(trimmed);

                if trimmed.eq_ignore_ascii_case("exit") || trimmed.eq_ignore_ascii_case("quit") {
                    println!("Bye!");
                    break;
                }

                execute_command(&client, trimmed).await;
            }
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                println!("\nBye!");
                break;
            }
            Err(err) => {
                eprintln!("Error: {:?}", err);
                break;
            }
        }
    }

    Ok(())
}

async fn execute_command(client: &AeroClient, line: &str) {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.is_empty() {
        return;
    }

    let cmd = parts[0].to_uppercase();

    match cmd.as_str() {
        "PING" => match client.ping().await {
            Ok(_) => println!("PONG"),
            Err(e) => eprintln!("(error) {}", e),
        },

        "GET" => {
            if parts.len() < 2 {
                eprintln!("(error) ERR wrong number of arguments for 'get' command. Usage: GET <key>");
                return;
            }
            match client.get(parts[1]).await {
                Ok(Some(val)) => match std::str::from_utf8(&val) {
                    Ok(s) => println!("\"{}\"", s),
                    Err(_) => println!("{:?}", val),
                },
                Ok(None) => println!("(nil)"),
                Err(e) => eprintln!("(error) {}", e),
            }
        }

        "SET" => {
            if parts.len() < 3 {
                eprintln!("(error) ERR wrong number of arguments for 'set' command. Usage: SET <key> <value>");
                return;
            }
            let key = parts[1];
            let val = Bytes::from(parts[2..].join(" "));
            match client.set(key, val, None).await {
                Ok(_) => println!("OK"),
                Err(e) => eprintln!("(error) {}", e),
            }
        }

        "EXP" => {
            if parts.len() < 4 {
                eprintln!("(error) ERR wrong number of arguments for 'exp' command. Usage: EXP <key> <ttl_ms> <value>");
                return;
            }
            let key = parts[1];
            let ttl_ms: u64 = match parts[2].parse() {
                Ok(t) => t,
                Err(_) => {
                    eprintln!("(error) ERR value is not a valid integer TTL");
                    return;
                }
            };
            let val = Bytes::from(parts[3..].join(" "));
            match client.set(key, val, Some(Duration::from_millis(ttl_ms))).await {
                Ok(_) => println!("OK"),
                Err(e) => eprintln!("(error) {}", e),
            }
        }

        "SETNX" => {
            if parts.len() < 3 {
                eprintln!("(error) ERR wrong number of arguments for 'setnx' command. Usage: SETNX <key> <value> [ttl_ms]");
                return;
            }
            let key = parts[1];
            let ttl = if parts.len() >= 4 {
                parts[3].parse::<u64>().ok().map(Duration::from_millis)
            } else {
                None
            };
            let val = Bytes::from(parts[2].to_string());
            match client.setnx(key, val, ttl).await {
                Ok(true) => println!("(integer) 1"),
                Ok(false) => println!("(integer) 0"),
                Err(e) => eprintln!("(error) {}", e),
            }
        }

        "DEL" | "DELETE" => {
            if parts.len() < 2 {
                eprintln!("(error) ERR wrong number of arguments for 'del' command. Usage: DEL <key>");
                return;
            }
            match client.delete(parts[1]).await {
                Ok(true) => println!("(integer) 1"),
                Ok(false) => println!("(integer) 0"),
                Err(e) => eprintln!("(error) {}", e),
            }
        }

        "INCR" => {
            if parts.len() < 2 {
                eprintln!("(error) ERR wrong number of arguments for 'incr' command. Usage: INCR <key> [delta]");
                return;
            }
            let delta = if parts.len() >= 3 {
                parts[2].parse::<i64>().unwrap_or(1)
            } else {
                1
            };
            match client.incr(parts[1], delta).await {
                Ok(n) => println!("(integer) {}", n),
                Err(e) => eprintln!("(error) {}", e),
            }
        }

        "DECR" => {
            if parts.len() < 2 {
                eprintln!("(error) ERR wrong number of arguments for 'decr' command. Usage: DECR <key> [delta]");
                return;
            }
            let delta = if parts.len() >= 3 {
                parts[2].parse::<i64>().unwrap_or(1)
            } else {
                1
            };
            match client.incr(parts[1], -delta).await {
                Ok(n) => println!("(integer) {}", n),
                Err(e) => eprintln!("(error) {}", e),
            }
        }

        "TTL" => {
            if parts.len() < 2 {
                eprintln!("(error) ERR wrong number of arguments for 'ttl' command. Usage: TTL <key>");
                return;
            }
            match client.ttl(parts[1]).await {
                Ok(t) => println!("(integer) {}", t),
                Err(e) => eprintln!("(error) {}", e),
            }
        }

        "EXISTS" => {
            if parts.len() < 2 {
                eprintln!("(error) ERR wrong number of arguments for 'exists' command. Usage: EXISTS <key>");
                return;
            }
            match client.exists(parts[1]).await {
                Ok(true) => println!("(integer) 1"),
                Ok(false) => println!("(integer) 0"),
                Err(e) => eprintln!("(error) {}", e),
            }
        }

        "PROTO" | "PROTOCOL" => {
            println!("Active Transport Protocol: {}", client.active_protocol());
        }

        "STATS" => match client.stats().await {
            Ok(s) => {
                println!("--- AeroKV Statistics ---");
                println!("Total Keys:      {}", s.key_count);
                println!("Used Memory:     {} bytes ({:.2} MB)", s.used_memory_bytes, s.used_memory_bytes as f64 / (1024.0 * 1024.0));
                println!("Hits:            {}", s.hits);
                println!("Misses:          {}", s.misses);
                println!("Expired Keys:    {}", s.expired_keys);
                println!("Evicted Keys:    {}", s.evicted_keys);
            }
            Err(e) => eprintln!("(error) {}", e),
        },

        "FLUSH" | "FLUSHALL" => match client.flush().await {
            Ok(_) => println!("OK"),
            Err(e) => eprintln!("(error) {}", e),
        },

        "HELP" => {
            println!("Available Commands:");
            println!("  PING                     Check server connectivity");
            println!("  GET <key>                Fetch value for key");
            println!("  SET <key> <value>        Store key-value pair");
            println!("  EXP <key> <ttl_ms> <val> Store key-value with millisecond TTL");
            println!("  SETNX <key> <val> [ttl]  Set if not exists");
            println!("  DEL <key>                Delete key");
            println!("  INCR <key> [delta]       Increment integer value");
            println!("  DECR <key> [delta]       Decrement integer value");
            println!("  TTL <key>                Get remaining TTL in milliseconds");
            println!("  EXISTS <key>             Check if key exists");
            println!("  PROTO                    Show active transport protocol");
            println!("  STATS                    Print memory and hit rate metrics");
            println!("  FLUSH                    Wipe all database keys");
            println!("  EXIT / QUIT              Exit console");
        }

        _ => {
            eprintln!("(error) ERR unknown command '{}', try HELP", parts[0]);
        }
    }
}
