//! CLI argument parsing.

use clap::Parser;

/// Blackjack TVC enclave HTTP server
#[derive(Parser, Debug)]
#[command(name = "blackjack", version, about = "Blackjack REST server for TVC enclave")]
pub struct Cli {
    /// IP address to listen on
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// Port to listen on
    #[arg(long, default_value = "44020")]
    pub port: u16,
}
