//! Dev tool: runs the in-memory WebDAV server on a port, optionally seeding one CBZ, so the app's
//! NAS screens can be tried without a NAS.
//! `cargo run -p tm-remote --features fake-server --example fake_dav -- 8765 "<path on server>" <file.cbz>`
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let port: u16 = args.first().and_then(|p| p.parse().ok()).unwrap_or(8765);
    let srv = tm_remote::fake::start_on(port).await;
    if let (Some(path), Some(file)) = (args.get(1), args.get(2)) {
        srv.put_file(path, &std::fs::read(file).expect("seed file"));
    }
    println!("WebDAV at {}", srv.base);
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
    }
}
