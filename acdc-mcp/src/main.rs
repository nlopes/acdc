#![deny(unreachable_pub)]

mod config;
mod document;
mod engine;
mod error;
mod paging;
mod render;
mod server;

use clap::Parser;
use rmcp::{ServiceExt, transport::stdio};

#[tokio::main(worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = config::Cli::parse().into_config()?;
    let service = server::Reader::new(config).serve(stdio()).await?;
    let cancellation = service.cancellation_token();
    tokio::select! {
        result = service.waiting() => { result?; }
        result = tokio::signal::ctrl_c() => { result?; cancellation.cancel(); }
    }
    Ok(())
}
