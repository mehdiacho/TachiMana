//! The client against a scripted HTTP server: auth header, JSON decoding, error mapping.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::{Arc, Mutex};

use tm_exthost::{Client, Error};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Serves one canned response per connection and records each request head.
async fn serve(responses: Vec<String>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    tokio::spawn(async move {
        for resp in responses {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 16 * 1024];
            let n = sock.read(&mut buf).await.unwrap();
            log.lock()
                .unwrap()
                .push(String::from_utf8_lossy(&buf[..n]).to_string());
            sock.write_all(resp.as_bytes()).await.unwrap();
            sock.shutdown().await.ok();
        }
    });
    (format!("http://{addr}"), seen)
}

fn http(status: &str, ctype: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[tokio::test]
async fn sends_the_token_and_decodes_big_source_ids() {
    let body = r#"[{"id":2499283573021220255,"name":"MangaDex","lang":"en","extensionPkg":"p","supportsLatest":true,"isConfigurable":true}]"#;
    let (base, seen) = serve(vec![http("200 OK", "application/json", body)]).await;
    let client = Client::new(base, "secret-token").unwrap();
    let sources = client.sources().await.unwrap();
    assert_eq!(sources[0].id, 2_499_283_573_021_220_255);
    let head = seen.lock().unwrap()[0].to_lowercase();
    assert!(head.starts_with("get /v1/sources "));
    assert!(head.contains("authorization: bearer secret-token"));
}

#[tokio::test]
async fn host_errors_keep_kind_and_message() {
    let err = r#"{"error":"Source 1 is not installed","kind":"notFound"}"#;
    let (base, _) = serve(vec![
        http("404 Not Found", "application/json", err),
        http("502 Bad Gateway", "text/plain", "upstream broke"),
    ])
    .await;
    let client = Client::new(base, "t").unwrap();
    match client.popular(1, 1).await {
        Err(Error::Host {
            status,
            kind,
            message,
        }) => {
            assert_eq!(
                (status, kind.as_str(), message.as_str()),
                (404, "notFound", "Source 1 is not installed")
            );
        }
        other => panic!("unexpected {other:?}"),
    }
    let e = client.popular(1, 1).await.unwrap_err();
    assert_eq!(
        e.kind(),
        "source",
        "non-JSON error bodies still map to a kind"
    );
}

#[tokio::test]
async fn search_encodes_the_query_and_clamps_the_page() {
    let (base, seen) = serve(vec![http(
        "200 OK",
        "application/json",
        r#"{"mangas":[{"url":"/m","title":"T"}],"hasNextPage":false}"#,
    )])
    .await;
    let client = Client::new(base, "t").unwrap();
    let page = client.search(7, "one piece & co", 0).await.unwrap();
    assert_eq!(page.mangas[0].update_strategy, "ALWAYS_UPDATE");
    let head = seen.lock().unwrap()[0].clone();
    assert!(
        head.starts_with("GET /v1/sources/7/search?q=one%20piece%20%26%20co&page=1 "),
        "{head}"
    );
}

#[tokio::test]
async fn images_carry_their_content_type() {
    let (base, seen) = serve(vec![http("200 OK", "image/png", "\u{89}PNG")]).await;
    let client = Client::new(base, "t").unwrap();
    let img = client.image_url(3, "https://x/c.png").await.unwrap();
    assert_eq!(img.content_type, "image/png");
    assert!(!img.bytes.is_empty());
    assert!(seen.lock().unwrap()[0].contains(r#"{"url":"https://x/c.png"}"#));
}

#[tokio::test]
async fn garbage_json_is_a_decode_error() {
    let (base, _) = serve(vec![http("200 OK", "application/json", "{nope")]).await;
    let client = Client::new(base, "t").unwrap();
    assert_eq!(client.health().await.unwrap_err().kind(), "decode");
}

#[tokio::test]
async fn unreachable_host_is_a_transport_error() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let client = Client::new(format!("http://127.0.0.1:{port}"), "t").unwrap();
    assert_eq!(client.health().await.unwrap_err().kind(), "transport");
}

#[tokio::test]
async fn filters_come_as_a_tree_and_an_older_helper_has_none() {
    let body = r#"[{"type":"header","name":"Note"},
        {"type":"select","name":"Status","values":["Any","Ongoing"],"state":0},
        {"type":"group","name":"Rating","filters":[{"type":"checkbox","name":"Safe","state":true}]},
        {"type":"sort","name":"Sort","values":["Latest"],"state":{"index":0,"ascending":false}},
        {"type":"future","name":"?","extra":1}]"#;
    let (base, seen) = serve(vec![
        http("200 OK", "application/json", body),
        http("404 Not Found", "text/plain", ""),
        http("500 Internal Server Error", "text/plain", "boom"),
    ])
    .await;
    let client = Client::new(base, "t").unwrap();
    let filters = client.filters(7).await.unwrap();
    assert_eq!(filters.len(), 5);
    assert_eq!(filters[1].kind, "select");
    assert_eq!(filters[1].values.as_deref().unwrap(), ["Any", "Ongoing"]);
    assert_eq!(filters[2].filters.as_ref().unwrap()[0].name, "Safe");
    assert_eq!(filters[4].kind, "future", "an unknown kind keeps its place");
    assert!(seen.lock().unwrap()[0].starts_with("GET /v1/sources/7/filters "));
    assert!(
        client.filters(7).await.unwrap().is_empty(),
        "a helper without the route means no filters"
    );
    assert!(client.filters(7).await.is_err(), "other failures are shown");
}

#[tokio::test]
async fn a_filtered_search_posts_the_states_in_order() {
    let (base, seen) = serve(vec![http(
        "200 OK",
        "application/json",
        r#"{"mangas":[],"hasNextPage":false}"#,
    )])
    .await;
    let client = Client::new(base, "t").unwrap();
    let states = serde_json::json!([null, 1, [true, false]]);
    let page = client.search_filtered(7, "", 0, &states).await.unwrap();
    assert!(page.mangas.is_empty());
    let req = seen.lock().unwrap()[0].clone();
    assert!(req.starts_with("POST /v1/sources/7/search "), "{req}");
    let body: serde_json::Value =
        serde_json::from_str(req.split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"q": "", "page": 1, "filters": [null, 1, [true, false]]})
    );
}
