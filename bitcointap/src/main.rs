use bitcointap::RuntimeError;
use shared::log;

/*
#![cfg_attr(feature = "strict", deny(warnings))]

/// The peer-observer extractor hooks into a Bitcoin Core binary with
/// tracepoints and publishes events into a NATS pub-sub queue.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Address of the NATS server where the extractor will publish messages to.
    #[arg(short, long, default_value = "127.0.0.1:4222")]
    nats_address: String,

    /// Path to the Bitcoin Core (bitcoind) binary that should be hooked into.
    #[arg(short, long)]
    bitcoind_path: String,

    /// PID (Process ID) of the Bitcoin Core (bitcoind) binary that should be hooked into.
    /// If this is set, the --bitcoind-pid-file argument isn't used.
    // TODO: remove the default value once https://github.com/bitcoin/bitcoin/pull/26593 is merged
    #[arg(long, default_value_t = DEFAULT_PID)]
    bitcoind_pid: i32,

    /// File containing the PID (Process ID) of the Bitcoin Core (bitcoind) binary that should be hooked into.
    /// If --bitcoind-pid is set, this flag is ignored.
    #[arg(long, default_value = "")]
    bitcoind_pid_file: String,

    // Default tracepoints
    /// Controls if the p2p message tracepoints should be hooked into.
    #[arg(long)]
    no_p2pmsg_tracepoints: bool,
    /// Controls if the connection tracepoints should be hooked into.
    #[arg(long)]
    no_connection_tracepoints: bool,
    /// Controls if the mempool tracepoints should be hooked into.
    #[arg(long)]
    no_mempool_tracepoints: bool,
    /// Controls if the validation tracepoints should be hooked into.
    #[arg(long)]
    no_validation_tracepoints: bool,

    // Custom tracepoints
    /// Controls if the addrman tracepoints should be hooked into.
    /// These may not have been PRed to Bitcoin Core yet.
    #[arg(long)]
    addrman_tracepoints: bool,

    /// The log level the extractor should run with. Valid log levels are "trace",
    /// "debug", "info", "warn", "error". See https://docs.rs/log/latest/log/enum.Level.html
    #[arg(short, long, default_value_t = log::Level::Debug)]
    log_level: log::Level,

    /// If used, libbpf will print debug information about the BPF maps,
    /// programs, and tracepoints during extractor startup. This can be
    /// useful during debugging.
    #[arg(long, default_value_t = false)]
    libbpf_debug: bool,
}
*/

fn run() -> Result<(), RuntimeError> {
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        log::error!("Fatal error during extractor runtime: {}", e);
    }
}
