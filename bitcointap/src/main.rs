#![cfg_attr(feature = "strict", deny(warnings))]

use bitcointap::{BitcoinTap, PidSource, RuntimeError, TapMsg};
use clap::{Parser, arg, command};
use shared::log;
use std::path::PathBuf;

/// The peer-observer extractor hooks into a Bitcoin Core binary with
/// tracepoints and publishes events into a NATS pub-sub queue.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the Bitcoin Core (bitcoind) binary that should be hooked into.
    #[arg(short, long, default_value = "")]
    bitcoind_path: String,

    /// PID (Process ID) of the Bitcoin Core (bitcoind) binary that should be hooked into.
    /// If this is set, the --bitcoind-pid-file argument isn't used.
    // TODO: remove the default value once https://github.com/bitcoin/bitcoin/pull/26593 is merged
    #[arg(long, default_value_t = -1)]
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

    let (pid_src, path) = find_bitcoind_and_pid(&args)?;

    log::info!("using pid source {:?}", &pid_src);

    let mut tap = BitcoinTap::new(path)
        .pid_source(pid_src)
        .debug(args.libbpf_debug)
        .attach()?;

    while let Ok(ev) = &tap.events().recv() {
        match ev {
            TapMsg::Event(ev) => println!("{}", serde_json::to_string(ev).expect("json msg")),
            TapMsg::Error(err) => log::error!("{err}"),
            TapMsg::Detached => break,
        }
    }

    log::info!("DONE!");

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        log::error!("Fatal error during extractor runtime: {}", e);
    }
}

fn get_running_bitcoind_pid_and_path() -> Option<(i32, PathBuf)> {
    let output = std::process::Command::new("pgrep")
        .arg("-a")
        .arg("bitcoin")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let mut parts = line.split_whitespace();
        let pid_str = parts.next()?;
        let pid: i32 = pid_str.parse().ok()?;

        for part in parts.clone() {
            if part.contains("bitcoin") {
                return Some((pid, PathBuf::from(part)));
            }
        }
    }

    None
}

fn find_bitcoind_and_pid(args: &Args) -> Result<(PidSource, PathBuf), RuntimeError> {
    let pid_src = match args.bitcoind_pid {
        -1 => {
            if args.bitcoind_pid_file != "" {
                PidSource::File(PathBuf::from(&args.bitcoind_pid_file))
            } else {
                PidSource::DefaultPid
            }
        }
        pid => PidSource::Manual(pid),
    };

    let detected = get_running_bitcoind_pid_and_path();

    let pid_src = if let PidSource::DefaultPid = pid_src {
        if let Some((detected_pid, _)) = &detected {
            PidSource::Manual(*detected_pid)
        } else {
            PidSource::DefaultPid
        }
    } else {
        pid_src
    };

    let path = if args.bitcoind_path != "" {
        PathBuf::from(&args.bitcoind_path)
    } else {
        if let Some((_, detected_path)) = detected {
            detected_path
        } else {
            return Err(RuntimeError::MissingBitcoind);
        }
    };

    Ok((pid_src, path))
}
