pub mod auth;
pub mod memory;
pub mod protocol;
pub mod resp;
pub mod shard;

use auth::Authenticator;
use http_body_util::BodyExt;
use hyper_util::rt::TokioIo;
use protocol::RequestHandler;
use resp::run_resp_server;
use shard::{run_shard_worker, LocalShard, ShardCommand, ShardPool};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub http_port: u16,
    pub resp_port: u16,
    pub token: String,
    pub max_mem_mb: usize,
    pub num_cores: usize,
}

pub struct ServerHandle {
    pub http_addr: SocketAddr,
    pub resp_addr: SocketAddr,
    pub shutdown_tx: tokio::sync::broadcast::Sender<()>,
}

pub async fn start_server(
    config: ServerConfig,
) -> Result<ServerHandle, Box<dyn std::error::Error>> {
    let auth = Authenticator::new(&config.token);
    let core_ids = core_affinity::get_core_ids().unwrap_or_default();
    let per_shard_mem = (config.max_mem_mb * 1024 * 1024) / config.num_cores.max(1);
    let mut shard_senders = Vec::with_capacity(config.num_cores);

    for i in 0..config.num_cores {
        let (tx, rx) = mpsc::channel::<ShardCommand>(32_768);
        shard_senders.push(tx);

        let core_id = core_ids.get(i).cloned();
        std::thread::Builder::new()
            .name(format!("aerokv-shard-{i}"))
            .spawn(move || {
                if let Some(cid) = core_id {
                    let _ = core_affinity::set_for_current(cid);
                }
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();

                let shard = LocalShard::new(per_shard_mem);
                rt.block_on(run_shard_worker(shard, rx));
            })?;
    }

    let shard_pool = ShardPool::new(shard_senders);

    // Bind TCP Listener for HTTP/1.1 & HTTP/2
    let tcp_addr = SocketAddr::from(([0, 0, 0, 0], config.http_port));
    let tcp_listener = tokio::net::TcpListener::bind(tcp_addr).await?;
    let bound_http_addr = tcp_listener.local_addr()?;

    let handler = Arc::new(RequestHandler::new(
        auth.clone(),
        shard_pool.clone(),
    ));

    // Bind RESP TCP Listener
    let resp_addr = SocketAddr::from(([0, 0, 0, 0], config.resp_port));
    let resp_listener = tokio::net::TcpListener::bind(resp_addr).await?;
    let bound_resp_addr = resp_listener.local_addr()?;

    let (shutdown_tx, _shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);

    // Spawn RESP server
    let resp_pool = shard_pool.clone();
    let resp_auth = auth.clone();
    tokio::spawn(async move {
        run_resp_server(resp_listener, resp_auth, resp_pool).await;
    });

    // Accept TCP HTTP/1.1 & HTTP/2 connections
    let tcp_handler = Arc::clone(&handler);
    tokio::spawn(async move {
        while let Ok((stream, _)) = tcp_listener.accept().await {
            let _ = stream.set_nodelay(true);
            let handler = Arc::clone(&tcp_handler);
            let io = TokioIo::new(stream);

            tokio::spawn(async move {
                let service = hyper::service::service_fn(
                    move |req: hyper::Request<hyper::body::Incoming>| {
                        let handler = Arc::clone(&handler);
                        async move {
                            let (parts, body) = req.into_parts();
                            let collected = body
                                .collect()
                                .await
                                .map(|c| c.to_bytes())
                                .unwrap_or_default();
                            let req = http::Request::from_parts(parts, collected);
                            let resp = handler.handle_request(req).await;
                            Ok::<_, std::convert::Infallible>(resp)
                        }
                    },
                );

                let _ = hyper_util::server::conn::auto::Builder::new(
                    hyper_util::rt::TokioExecutor::new(),
                )
                .serve_connection(io, service)
                .await;
            });
        }
    });

    Ok(ServerHandle {
        http_addr: bound_http_addr,
        resp_addr: bound_resp_addr,
        shutdown_tx,
    })
}
