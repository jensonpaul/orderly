
# orderly

A Rust CLI WebSocket client for crypto exchanges. 
Connects to the WebSocket feeds of multiple exchanges. 
Subscribes to the live order book for the given currency pair.
Publishes a merged order book as a gRPC stream.

<img src="https://user-images.githubusercontent.com/1086619/170038125-8a4ed933-9cec-4a7a-9085-2dd806ca0307.gif" />

Currently supports: 

* Bitstamp WebSocket: `wss://ws.bitstamp.net`
* Binance WebSocket: `wss://stream.binance.com:9443/ws`
* Kraken WebSocket: `wss://ws.kraken.com`
* Coinbase WebSocket: `wss://ws-feed.exchange.coinbase.com`

```
USAGE:
    orderly-server [OPTIONS]

OPTIONS:
    -h, --help               Print help information
        --no-binance         (Optional) Don't show Binance in gRPC stream. Default: false
        --no-bitstamp        (Optional) Don't show Bitstamp in gRPC stream. Default: false
        --no-kraken          (Optional) Don't show Kraken in gRPC stream. Default: false
        --no-coinbase        (Optional) Don't show Coinbase in gRPC stream. Default: false
    -p, --port <PORT>        (Optional) Port number on which the the gRPC server will be hosted.
                             Default: 50051
    -s, --symbol <SYMBOL>    (Optional) Currency pair to subscribe to. Default: ETH/BTC
```

Run gRPC server:

```
cargo run --bin orderly-server
```
or with logs and options:
```
env RUST_LOG=info cargo run --bin orderly-server -- --symbol ETH/BTC --port 50051
```
Exclude certain exchanges:

```
cargo run --bin orderly-server -- --no-binance --no-bitstamp
```

Client
-----

Connects to the gRPC server and streams the orderbook summary.

<img src="https://user-images.githubusercontent.com/1086619/169551698-3d59df5d-73db-47a3-a84d-cb0d2d0dd678.jpg" width="700"/>

```
USAGE:
    orderly-client [OPTIONS]

OPTIONS:
    -h, --help           Print help information
    -p, --port <PORT>    (Optional) Port number of the gRPC server. Default: 50051
```

Run gRPC client:

```
cargo run --bin orderly-client
```
or with logs and options:

```
env RUST_LOG=info cargo run --bin orderly-client -- --port 50051
```

---

Daemon
-----

For a production-quality CentOS 9 systemd service running your Rust gRPC server, you'll generally want:

* Dedicated service user
* Fixed working directory
* Environment variables managed separately
* Automatic restart on failure
* Resource limits
* Logging to journald
* Security hardening
* Clean shutdown support

Example assuming:

* Binary: `/opt/orderly/bin/orderly-server`
* User: `orderly`
* Data/config directory: `/opt/orderly`
* Listen port: `50051`
* Symbol: `BTC/USD`

First create the service account:

```bash
sudo useradd \
  --system \
  --home-dir /opt/orderly \
  --shell /sbin/nologin \
  orderly
```

Install your binary:

```bash
sudo mkdir -p /opt/orderly/bin
sudo cp target/release/orderly-server /opt/orderly/bin/
sudo chown -R orderly:orderly /opt/orderly
```

Create an environment file:

```bash
sudo mkdir -p /etc/orderly
sudo vi /etc/orderly/orderly.env
```

Contents:

```bash
RUST_LOG=info
SYMBOL=BTC/USD
PORT=50051
```

Create the systemd unit:

`/etc/systemd/system/orderly-server.service`

```ini
[Unit]
Description=Orderly Rust gRPC Server
After=network-online.target
Wants=network-online.target

[Service]
Type=simple

User=orderly
Group=orderly

WorkingDirectory=/opt/orderly

EnvironmentFile=/etc/orderly/orderly.env

Environment="RUST_LOG=info"
Environment="SYMBOL=BTC/USD"
Environment="PORT=50051"

#ExecStart=/opt/orderly/bin/orderly-server \
#    --symbol ${SYMBOL} \
#    --port ${PORT}
#ExecStart=/bin/bash -lc '/opt/orderly/bin/orderly-server --symbol "$SYMBOL" --port "$PORT"'

ExecStart=/bin/bash -lc '/opt/orderly/bin/orderly-server --symbol BTC/USD --port 50051'

Restart=on-failure
RestartSec=5

# Increase if your service opens many sockets/files
LimitNOFILE=65535

# Send logs to journald
StandardOutput=journal
StandardError=journal

# Security hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
RestrictSUIDSGID=true
LockPersonality=true
MemoryDenyWriteExecute=true

# Writable paths if needed
ReadWritePaths=/opt/orderly

# Graceful shutdown
TimeoutStopSec=30
KillSignal=SIGINT

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo systemctl daemon-reload

sudo systemctl enable orderly-server

sudo systemctl start orderly-server
```

Check status:

```bash
sudo systemctl status orderly-server
```

View logs:

```bash
journalctl -u orderly-server -f
```

If you want to run exactly the equivalent of:

```bash
RUST_LOG=info cargo run --bin orderly-server -- \
    --symbol BTC/USD \
    --port 50051
```

during development (not recommended for production), use:

```ini
ExecStart=/usr/bin/bash -c '
export RUST_LOG=info
cargo run --bin orderly-server -- \
  --symbol "BTC/USD" \
  --port 50051
'
```

However, for production on CentOS 9, build with:

```bash
cargo build --release --bin orderly-server
```

and run the compiled binary directly, as shown in the first service definition. This avoids needing Cargo, Rust toolchains, and source code on the production host.
