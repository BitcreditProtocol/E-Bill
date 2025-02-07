# Configuration

The application can be configured using command-line parameters, or environment variables

The following options are available:

* `P2P_PORT` / `--p2p-port` - the P2P port (default: 1908)
* `P2P_ADDRESS` / `--p2p-address` - the P2P address (default: 0.0.0.0)
* `HTTP_PORT` / `--http-port` - the HTTP address (default: 8000)
* `HTTP_ADDRESS` / `--http-address` - the HTTP address (default: 127.0.0.1)
* `DATA_DIR` - the data directory root (default: ".")
* `SURREAL_DB_CONNECTION` - the surreal DB connection (default: "ws://localhost:8800") - set to `rocksdb://data/surreal` for embedded mode
* `TERMINAL_CLIENT` - whether to start a debug terminal client (default: false)
* `ENVIRONMENT` - environment to start the app in (default: development)
* `RUST_LOG` - the log level, e.g.: info, trace, debug, error (default: error)
* `NOSTR_RELAY` - nostr relay endpoint (default: ws://localhost:8080)
* `MINT_URL` - cashu mint endpoint (default: https://moksha.minibill.tech)
* `JOB_RUNNER_INITIAL_DELAY_SECONDS` - initial delay until cron jobs run (default: 1)
* `JOB_RUNNER_CHECK_INTERVAL_SECONDS` - interval in which cron jobs run (default: 600)
* `FRONTEND_URL_PATH` - default path to serve the frontend from (default: /)
* `FRONTEND_SERVE_FOLDER` - folder where the static frontend is served from (default: ./frontend)
* `LAUNCH_FRONTEND_AT_STARTUP` - open the frontend in a browser on startup (default: false)
* `RELAY_BOOTSTRAP_ADDRESS` - the relay bootstrap address (default: /ip4/45.147.248.87/tcp/1908)
* `RELAY_BOOTSTRAP_PEER_ID` - the relay bootstrap peer id (default: 12D3KooWL5y2jyVFtk541g9ySSoKGjNf61GEPG1XbPhop5MRfyA8)

## Example

```bash
RUST_LOG=info cargo run -- --http-port 8001

RUST_LOG=info HTTP_PORT=8001 cargo run
```

