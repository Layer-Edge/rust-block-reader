use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, ValueEnum)]
pub enum Mode {
    /// Run in Rest API Mode where it will receive block number and forward block hash to aggregator
    REST,
    /// Run in Loop Mode where it will fetch latest block hash and forward it to aggregator
    LOOP,
    /// Run both Rest API and Loop Mode parallel
    BOTH,
}

#[derive(Parser, Debug)]
#[command(name = "Layeredge Block Reader")]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Run Server in Different Modes
    #[arg(long, short, value_enum, default_value_t = Mode::REST)]
    pub mode: Mode,
}
