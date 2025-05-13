# bitcointap

A rust library and cli tool for tapping into bitcoin-core tracepoints to
extract data in realtime.

## Status

This is currently a WORK IN PROGRESS, but it works(ish)!

- [x] switch from nats to mpsc channel
- [x] cli tool with json events
- [x] json logger from channel
- [ ] event selection api
- [ ] event selection from cli
- [ ] msgpack logger from channel

## Usage

```bash
$ bitcointap mempool:added 
{"txid":"b89b342d8be5d07ca41f6f9c5c8a3b9b7a3262f0e802910ccc79f89bf4b625fa", ...}
{"txid":"1013d5ceaac615e7ac330b3422530d58d49ccef1cff10cf2a89615be91cea27e", ...}
...
```

## How it works

`bitcointap` is written in Rust and uses the Bitcoin Core tracepoints to extract
events like received and send P2P messages, open and closed P2P connections, mempool
changes, and more. This is implemented using the USDT capabilites of [libbpf-rs].
The Bitcoin P2P protocol messages are deserialized using [rust-bitcoin].
```
              Tracepoints
┌───────────┐ via libbpf
│  Bitcoin  │          ┌────────────┐
│ Core Node ├──────────► bitcointap ├────────►msgpack or json
└───────────┘          └────────────┘
```

## Credits

Most of the hard work here was done by @0xb10c's [peer-observer]. I am simply
swapping out the protobuf and nats code with json and stdout.

[peer-observer]: https://github.com/0xB10C/peer-observer
[msgpack]: https://msgpack.org/
[libbpf-rs]: https://github.com/libbpf/libbpf-rs
[rust-bitcoin]: https://github.com/rust-bitcoin/rust-bitcoin
