//! healthchecks.io pings against a local mock HTTP server (#155; docs/server.md, Logs and
//! monitoring).

use std::net::SocketAddr;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use iwbtg_server::health;

/// A minimal HTTP server that answers `responses` GETs with 200 and reports each raw request.
async fn mock_server(responses: usize) -> (SocketAddr, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind a mock server");
    let addr = listener.local_addr().expect("mock address");
    let (tx, rx) = mpsc::channel(responses + 1);
    tokio::spawn(async move {
        for _ in 0..responses {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut buffer = vec![0u8; 4096];
            let read = socket.read(&mut buffer).await.unwrap_or(0);
            let request = String::from_utf8_lossy(&buffer[..read]).into_owned();
            let _ = tx.send(request).await;
            let _ = socket
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                .await;
            let _ = socket.shutdown().await;
        }
    });
    (addr, rx)
}

#[tokio::test]
async fn a_ping_reaches_the_configured_url() {
    let (addr, mut requests) = mock_server(1).await;

    let status = health::ping_once(&format!("http://{addr}/ping/abc-123"))
        .await
        .expect("the mock answers");
    assert_eq!(status, 200);

    let request = requests.recv().await.expect("the mock received the ping");
    assert!(
        request.starts_with("GET /ping/abc-123 HTTP/1.1\r\n"),
        "unexpected request line: {request}"
    );
    assert!(
        request.to_lowercase().contains(&format!("host: {addr}")),
        "no Host header: {request}"
    );
}

/// healthchecks.io being down, the network being down and a typo in the URL all take the same
/// path: an error the loop logs.
#[tokio::test]
async fn an_unreachable_url_is_an_error_not_a_panic() {
    let err = health::ping_once("http://127.0.0.1:1/never-listens")
        .await
        .expect_err("connecting to a closed port must fail");
    assert!(!err.is_empty(), "the error has no message");
}

#[tokio::test]
async fn a_malformed_url_is_an_error_not_a_panic() {
    assert!(health::ping_once("not a url").await.is_err());
}

/// The scheduler pings immediately and then once per period, so a restart shows up at once.
#[tokio::test]
async fn the_loop_pings_startup_and_then_every_period() {
    let (addr, mut requests) = mock_server(2).await;
    let task = tokio::spawn(health::ping_forever(
        format!("http://{addr}/ping"),
        Duration::from_millis(20),
    ));

    for ping in ["startup", "second"] {
        let request = tokio::time::timeout(Duration::from_secs(5), requests.recv())
            .await
            .unwrap_or_else(|_| panic!("no {ping} ping"))
            .expect("the mock received the ping");
        assert!(request.starts_with("GET /ping "), "{ping}: {request}");
    }

    task.abort();
}

/// A failed ping is logged, not fatal: the loop must still be running afterwards.
#[tokio::test]
async fn a_failing_loop_keeps_running() {
    let task = tokio::spawn(health::ping_forever(
        "http://127.0.0.1:1/never-listens".to_owned(),
        Duration::from_millis(20),
    ));

    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(!task.is_finished(), "a failed ping stopped the monitoring loop");

    task.abort();
}
