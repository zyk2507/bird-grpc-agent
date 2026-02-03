# BIRD gRPC Agent

This is a standalone gRPC exporter for the BIRD daemon. It reads the shared
memory (SHM) snapshot published by BIRD and exposes it over gRPC.

## Requirements

- BIRD must be built with SHM export enabled and started with
  `ENABLE_SHM_EXPORT` set to a non-zero integer.
- The SHM segment is created by BIRD at `"/bird_shm_export"`.

## Build

```
cargo check
cargo build
```

## Run

```
BIRD_GRPC_ADDR=127.0.0.1:50051 ./target/debug/bird-grpc-agent
```

## Concurrency and Single Instance

- All gRPC requests are handled **one at a time** to respect BIRD’s single
  mailbox request model.
- The agent enforces a **single running instance** using a lock file. If another
  instance is already running, the new process exits immediately.

Lock file location:
- `/run/bird-grpc-agent/bird-grpc-agent.lock` (fallbacks: `/var/run` or `/tmp`).

## systemd

A root-safe unit file is provided at:

```
packaging/systemd/bird-grpc-agent.service
```

Example environment file:

```
packaging/config/agent.env
```

## gRPC API

The protobuf definition is in `proto/exporter.proto` with package name
`birdexporter`.

RPCs:
- `GetStatus`
- `ListInterfaces`
- `ListProtocols`
- `ListBgp`
- `ListOspf`
- `ListBfd`
- `ListBabel`
- `GetSnapshot`
