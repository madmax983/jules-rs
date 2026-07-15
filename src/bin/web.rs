//! `jules-web` — cross-repo Jules session dashboard.
//!
//! Thin entry point: all logic lives in [`jules_rs::web`]. Configure the API
//! via `JULES_API_KEY` (required) and `JULES_API_URL` (optional). The listen
//! port is controlled by `autumn-web` (`AUTUMN_SERVER__PORT`, default `3000`).

#[autumn_web::main]
async fn main() {
    if let Err(error) = jules_rs::web::run().await {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}
