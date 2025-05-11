#![cfg_attr(feature = "strict", deny(warnings))]

use bitcointap::{BitcoinTap, PidSource, RuntimeError};
use clap::{Parser, arg, command};
use shared::log;
use std::path::PathBuf;

const DEFAULT_PID: i32 = -1;

/// The peer-observer extractor hooks into a Bitcoin Core binary with
/// tracepoints and publishes events into a NATS pub-sub queue.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
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

    /// If used, libbpf will print debug information about the BPF maps,
    /// programs, and tracepoints during extractor startup. This can be
    /// useful during debugging.
    #[arg(long, default_value_t = false)]
    libbpf_debug: bool,

    /// The log level the extractor should run with. Valid log levels are "trace",
    /// "debug", "info", "warn", "error". See https://docs.rs/log/latest/log/enum.Level.html
    #[arg(short, long, default_value_t = log::Level::Debug)]
    log_level: log::Level,
}

fn run() -> Result<(), RuntimeError> {
    let args = Args::parse();

    simple_logger::init_with_level(args.log_level)?;

    let pid_src = if args.bitcoind_pid == -1 {
        if args.bitcoind_pid_file != "" {
            PidSource::File(PathBuf::from(&args.bitcoind_pid_file))
        } else {
            PidSource::DefaultPid
        }
    } else {
        PidSource::Manual(args.bitcoind_pid)
    };

    log::info!("using pid source {:?}", &pid_src);

    let mut tap = BitcoinTap::new(args.bitcoind_path)
        .pid_source(pid_src)
        .debug(args.libbpf_debug)
        .attach()?;

    while let Ok(ev) = &tap.events().recv() {
        println!("{}", serde_json::to_string(ev).expect("json msg"));
    }

    log::info!("DONE!");

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        log::error!("Fatal error during extractor runtime: {}", e);
    }
}
