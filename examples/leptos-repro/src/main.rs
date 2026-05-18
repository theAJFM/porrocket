//! Minimal axum server — same hyper/tokio stack a Leptos SSR app uses.
//!
//! Reproduces the porrocket "Empty reply from server" failure.
//!
//! Usage:
//!     porrocket -p 4312 -u /tmp/leptos.sock -- ./target/release/leptos-repro 4312
//!
//! Test:
//!     curl --unix-socket /tmp/leptos.sock http://localhost/

use axum::{routing::get, Router};

#[tokio::main]
async fn main() {
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(4312);

    let app = Router::new().route("/", get(|| async { "hello from axum\n" }));

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("Server listening on {addr}");

    axum::serve(listener, app).await.unwrap();
}
