## Rust Block Reader

A lightweight Rust service for ingesting zk proofs and related on-chain signals (block hashes, headers, merkle roots, events) from multiple protocols, and preparing them for integration into LayerEdge's Aggregation & Verification Layer. It exposes functionality via:

- CLI modes: test, REST API server, background loop, or both
- Minimal HTTP endpoint to enqueue a block number and fetch its hash

This project uses async Rust with Tokio and supports fetching via JSON-RPC, SDK calls, contract event logs, and proof-related signals needed by verification pipelines.

### Features
- **Verification-focused ingestion**: reads zk-proof adjacent data (e.g., merkle roots, headers, events) across chains to feed LayerEdge's Verification Layer
- **Multiple modes** via `--mode` flag: `TEST`, `REST` (default), `LOOP`, `BOTH`
- **REST API**: `POST /add-block-by-number/{blockNumber}` on port `8080`
- **Loop mode**: periodically polls several configured chains/providers
- **Event mode**: reads latest `L2MerkleRootAdded` events for Linea

### Prerequisites
- Rust toolchain (Rust 1.75+ recommended). Install via `https://rustup.rs`.
- OpenSSL not required (uses `rustls`).
- ZeroMQ system library may be required for the `zmq` crate on some platforms:
  - macOS (Homebrew): `brew install zeromq`
  - Ubuntu/Debian: `sudo apt-get update && sudo apt-get install -y libzmq3-dev`

### Quick Start
```bash
git clone https://github.com/your-org/rust-block-reader.git
cd rust-block-reader
cargo run --release
```

By default, the service starts in `REST` mode and listens on `0.0.0.0:8080`.

### Configuration
The code currently uses several hardcoded endpoints and chain IDs inside `src/main.rs`. You can update these to match your environment (example snippet):

```rust
(
    "contract",
    "linea",
    59144,
    "https://0xrpc.io/eth",
    "L2MerkleRootAdded",
    "0xd19d4B5d358258f05D7B411E21A1460D11B0876F",
    None,
),
```

`.env` support is enabled via `dotenv`, but no specific variables are currently read in `main.rs`. You may add your own env reads where needed.

### Build
```bash
cargo build --release
```

### CLI Usage
The binary accepts a single `--mode` (or `-m`) flag. Default is `REST`.

```bash
cargo run -- --help
```

Example runs:

```bash
# REST API server only (default)
cargo run -- --mode REST

# Background loop only
cargo run -- --mode LOOP

# Run both REST and Loop concurrently
cargo run -- --mode BOTH

# Test mode (calls local function for testing)
cargo run -- --mode TEST
```

### REST API
- Base URL: `http://localhost:8080`
- Endpoint: `POST /add-block-by-number/{blockNumber}`
  - Example: `POST /add-block-by-number/12345`

Example with curl:

```bash
curl -X POST http://localhost:8080/add-block-by-number/12345
```

Successful response example:

```json
{"msg":"block hash added successfully","block_hash":"0x..."}
```

Error response example:

```json
{"error":"Failed to fetch block hash","details":"..."}
```

### Loop Mode
Loop mode iterates over a set of configured chains/providers every few seconds, then sleeps between cycles. The list is defined in `src/main.rs` under `block_fetch_params` and includes examples for:

- SDK-based fetch (Avail)
- RPC-based fetch (OnlyLayer, Mint, Bitfinity, U2U, Celestia, Kaanch)
- Contract event-based fetch (Linea `L2MerkleRootAdded`)

Adjust endpoints, chain IDs, and methods as needed for your environment.

### Project Structure
```rust
mod block_number_op;
mod block_reader;
mod cli_args;
mod merkle_root_op;
mod router;
mod rpc_call;
mod util;
```

- `src/cli_args.rs`: Defines the `Mode` enum and CLI parsing
- `src/main.rs`: Entry point, mode dispatch, REST server, loop logic
- `src/router.rs`: Minimal async router and route handling
- `src/block_reader.rs`: Core logic to fetch block hashes/events (see file)
- `src/block_number_op.rs`: Persist/read last processed block numbers
- `src/merkle_root_op.rs`: Merkle-root related helpers
- `src/rpc_call.rs`: RPC JSON calls
- `src/util.rs`: Utilities
- `src/404.html`: Simple 404 page served by the HTTP server

### Development
- Use `cargo fmt` and `cargo clippy` to maintain code quality
- Run in watch mode during development with `cargo watch -x run` (install `cargo-watch`)

### Troubleshooting
- If build fails with ZeroMQ-related errors, install the ZeroMQ system library (see Prerequisites)
- Network errors often indicate an invalid RPC URL or chain ID; verify and update the hardcoded values in `main.rs`

### License
Specify your project license here (e.g., MIT or Apache-2.0).