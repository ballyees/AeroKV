use aerokv_server::{start_server, ServerConfig};
use bytes::Bytes;
use core_client::AeroClient;

#[tokio::test]
async fn test_full_e2e_stack_and_resp() {
    let config = ServerConfig {
        http_port: 0, // OS assigns free port
        resp_port: 0, // OS assigns free port
        token: "test_secret_123".to_string(),
        max_mem_mb: 64,
        num_cores: 2,
    };

    let server_handle = start_server(config).await.expect("Failed to start server");
    let http_port = server_handle.http_addr.port();
    let resp_port = server_handle.resp_addr.port();

    println!("Testing against HTTP port {} and RESP port {}", http_port, resp_port);

    // 1. Test core-client against HTTP/TCP
    let client = AeroClient::connect("127.0.0.1", http_port, Some("test_secret_123"))
        .await
        .expect("Client connect failed");

    assert!(client.ping().await.unwrap());
    client.set("foo", Bytes::from("bar"), None).await.unwrap();
    assert_eq!(client.get("foo").await.unwrap(), Some(Bytes::from("bar")));

    assert_eq!(client.active_protocol(), "http");

    // Test SETNX, INCR, TTL, EXISTS, DELETE
    assert!(client.setnx("lock", Bytes::from("1"), None).await.unwrap());
    assert!(!client.setnx("lock", Bytes::from("2"), None).await.unwrap());
    assert_eq!(client.incr("counter", 10).await.unwrap(), 10);
    assert_eq!(client.incr("counter", -3).await.unwrap(), 7);
    assert!(client.exists("foo").await.unwrap());
    assert!(client.delete("foo").await.unwrap());
    assert!(!client.exists("foo").await.unwrap());

    // Test key prefix support
    let prefixed = client.with_prefix("tenant_a:");
    assert_eq!(prefixed.prefix(), Some("tenant_a:"));
    prefixed.set("user", Bytes::from("alice"), None).await.unwrap();
    assert_eq!(prefixed.get("user").await.unwrap(), Some(Bytes::from("alice")));
    // Verify on root client that underlying key is "tenant_a:user"
    assert_eq!(client.get("tenant_a:user").await.unwrap(), Some(Bytes::from("alice")));
    assert_eq!(client.get("user").await.unwrap(), None);
    assert!(prefixed.exists("user").await.unwrap());
    assert!(prefixed.delete("user").await.unwrap());
    assert!(!client.exists("tenant_a:user").await.unwrap());

    // 2. Test unauthorized access
    let unauth_client = AeroClient::connect("127.0.0.1", http_port, Some("wrong_token"))
        .await
        .expect("Client connect failed");
    assert!(unauth_client.get("lock").await.is_err());

    // 3. Test RESP TCP interface via redis crate
    let redis_url = format!("redis://:test_secret_123@127.0.0.1:{}", resp_port);
    let redis_client = redis::Client::open(redis_url).unwrap();
    let mut con = redis_client.get_multiplexed_async_connection().await.unwrap();

    let pong: String = redis::cmd("PING").query_async(&mut con).await.unwrap();
    assert_eq!(pong, "PONG");

    let _: () = redis::cmd("SET").arg("rkey").arg("rval").query_async(&mut con).await.unwrap();
    let val: String = redis::cmd("GET").arg("rkey").query_async(&mut con).await.unwrap();
    assert_eq!(val, "rval");

    let num: i64 = redis::cmd("INCRBY").arg("rcounter").arg(42).query_async(&mut con).await.unwrap();
    assert_eq!(num, 42);

    // Test RESP pipelining
    let mut pipe = redis::pipe();
    pipe.set("p1", "v1").set("p2", "v2").get("p1").get("p2");
    let ((), (), p1, p2): ((), (), String, String) = pipe.query_async(&mut con).await.unwrap();
    assert_eq!(p1, "v1");
    assert_eq!(p2, "v2");

    let _ = server_handle.shutdown_tx.send(());
}
