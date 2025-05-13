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
- [ ] rust lib docs
- [ ] rust lib example

## Usage

```bash
$ sudo bitcointap
{
  "timestamp": 1747158913,
  "timestamp_subsec_micros": 356600,
  "event": {
    "Msg": {
      "meta": {
        "peer_id": 11,
        "addr": "111.100.246.24:8333",
        "conn_type": 2,
        "command": "inv",
        "inbound": true,
        "size": 37
      },
      "msg": { "Inv": { "items": [ { "item": { "Wtx": [..] } } ] } }
    }
  }
}
{
  "timestamp": 1747158913,
  "timestamp_subsec_micros": 356608,
  "event": {
    "Msg": {
      "meta": {
        "peer_id": 11,
        "addr": "111.100.246.24:8333",
        "conn_type": 2,
        "command": "getdata",
        "inbound": false,
        "size": 37
      },
      "msg": { "Getdata": { "items": [ { "item": { "Wtx": [ ... ] } } ] } }
    }
  }
}
{
  "timestamp": 1747158913,
  "timestamp_subsec_micros": 408313,
  "event": {
    "Msg": {
      "meta": {
        "peer_id": 12,
        "addr": "bvymttfo4axni5n2lfu5y3ymhxmsmkatynmc4sthyutjujbyjft47cqd.onion:8333",
        "conn_type": 2,
        "command": "inv",
        "inbound": true,
        "size": 73
      },
      "msg": { "Inv": { "items": [ { "item": { "Wtx": [ .. ] } }, { "item": { "Wtx": [ ... ] } } ] }
      }
    }
  }
}
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
│ Core Node ├──────────► bitcointap ├────────►raw data/json
└───────────┘          └────────────┘
```

## Credits

Most of the hard work here was done by @0xb10c's [peer-observer]. I am simply
swapping out the protobuf and nats code with json and stdout.

[peer-observer]: https://github.com/0xB10C/peer-observer
[libbpf-rs]: https://github.com/libbpf/libbpf-rs
[rust-bitcoin]: https://github.com/rust-bitcoin/rust-bitcoin
