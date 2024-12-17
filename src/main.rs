use avail_rust::{hex, H256, SDK};
use clap::Parser;
use std::{
    fs,
    io::{Error, ErrorKind, Result},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::sleep,
};

mod cli_args;
mod router;
mod rpc_call;

use cli_args::{Args, Mode};
use router::Router;
use rpc_call::rpc::rpc_call;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = Args::parse();

    match args.mode {
        Mode::REST => rest_server().await?,
        Mode::LOOP => block_hash_loop().await?,
        Mode::BOTH => {
            if let Err(e) = tokio::try_join!(
                rest_server(),
                block_hash_loop(),
                block_hash_from_rpc_loop(
                    "onlylayer",
                    "https://onlylayer.org",
                    "eth_getBlockByNumber",
                    vec![
                        serde_json::Value::String("latest".to_string()),
                        serde_json::Value::Bool(false)
                    ]
                ),
                block_hash_from_rpc_loop(
                    "mintchain",
                    "https://global.rpc.mintchain.io",
                    "eth_getBlockByNumber",
                    vec![
                        serde_json::Value::String("latest".to_string()),
                        serde_json::Value::Bool(false)
                    ]
                ),
                block_hash_from_rpc_loop(
                    "bitfinity",
                    "https://mainnet.bitfinity.network",
                    "eth_getBlockByNumber",
                    vec![
                        serde_json::Value::String("latest".to_string()),
                        serde_json::Value::Bool(false)
                    ]
                ),
            ) {
                eprintln!("Error in BOTH mode: {}", e);
            }
        }
    }

    Ok(())
}

async fn rest_server() -> Result<()> {
    let mut router = Router::new();

    router.add_route(
        "/add-block-by-number/".to_string(),
        |block_number: String| async move {
            match fetch_block_hash("o3".to_string(), &block_number, None).await {
                Ok(block_hash) => {
                    format!(
                        "{{\"msg\": \"block hash added successfully\", \"block_hash\": \"0x{}\"}}",
                        hex::encode(block_hash.as_bytes())
                    )
                }
                Err(e) => format!(
                    "{{\"error\": \"Failed to fetch block hash\", \"details\": \"{}\"}}",
                    e.to_string()
                ),
            }
        },
    );

    let router = Arc::new(router);
    let listener = TcpListener::bind("0.0.0.0:8080").await?;
    println!("server is listening on 8080");
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let router_clone = Arc::clone(&router);
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, router_clone).await {
                        eprintln!("Connection handling error: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("Error accepting connection: {}", e);
            }
        }
    }
}

async fn block_hash_loop() -> Result<()> {
    let mut last_block_hash: Option<H256> = None;
    loop {
        match fetch_block_hash("avail".to_string(), "", last_block_hash).await {
            Ok(new_block_hash) => {
                last_block_hash = Some(new_block_hash);
            }
            Err(e) => println!("Failed to fetch block hash {:?}", e),
        }
        let _ = sleep(Duration::from_millis(10000));
    }
}

async fn block_hash_from_rpc_loop(
    chain_name: &str,
    rpc_url: &str,
    method: &str,
    params: Vec<serde_json::Value>,
) -> Result<()> {
    let zmq_socket_url =
        std::env::var("ZMQ_CHANNEL_URL").unwrap_or_else(|_| "tcp://0.0.0.0:40006".to_string());
    println!("zmq socket: {:?}", zmq_socket_url);
    let mut last_block_hash: Option<String> = None;
    loop {
        match rpc_call(rpc_url, method, params.clone()).await {
            Ok(new_block_hash) => {
                if let Some(latest_block_hash) = new_block_hash
                    .get("result")
                    .and_then(|result| result.get("hash"))
                    .and_then(|hash| hash.as_str())
                {
                    // Compare with the last block hash
                    if last_block_hash.as_deref() != Some(latest_block_hash) {
                        last_block_hash = Some(latest_block_hash.to_string());
                        println!("New block hash of {}: {}", chain_name, latest_block_hash);

                        let context = zmq::Context::new();
                        let sender = context
                            .socket(zmq::REQ)
                            .expect("Failed to create REQ socket");

                        sender
                            .connect(&zmq_socket_url)
                            .expect("failed to connect to endpoint");

                        let data: Vec<Vec<u8>> = vec![
                            b"datablock".to_vec(),
                            format!(
                                "{}-chain-{}",
                                chain_name,
                                hex::encode(last_block_hash.as_ref().unwrap()[2..].as_bytes())
                            )
                            .into_bytes(),
                            b"!!!!!".to_vec(),
                        ];

                        sender
                            .send_multipart(&data, 0)
                            .expect("failed to send data");
                        println!("data sent");
                    }
                } else {
                    eprintln!("Malformed response: {:?}", new_block_hash);
                }
            }
            Err(e) => println!("Failed to fetch block hash {:?}", e),
        }
        let _ = sleep(Duration::from_millis(1000));
    }
}

async fn handle_connection(mut stream: TcpStream, router: Arc<Router>) -> Result<()> {
    let mut buffer = [0; 1024];

    let bytes_read = stream.read(&mut buffer).await?;
    if bytes_read == 0 {
        return Err(Error::new(ErrorKind::UnexpectedEof, "Empty Request"));
    }

    let request = String::from_utf8_lossy(&buffer[..]);
    let path = request.split_whitespace().nth(1).unwrap_or("/");

    let (status_code, content_type, body) = match router.handle(path).await {
        Some(json_body) => ("200 OK", "application/json", json_body),
        None => (
            "404 NOT FOUND",
            "text/html",
            fs::read_to_string("src/404.html").unwrap(),
        ),
    };

    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n{}",
        status_code,
        content_type,
        body.len(),
        body
    );

    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;

    Ok(())
}

async fn fetch_block_hash(
    identifier: String,
    _block_number: &str,
    last_block_hash: Option<H256>,
) -> std::result::Result<H256, Box<dyn std::error::Error>> {
    let avail = SDK::new("wss://mainnet.avail-rpc.com/").await.unwrap();
    let block_number = if !_block_number.is_empty() {
        match _block_number.trim().parse::<u32>() {
            Ok(num) => Some(num),
            Err(_) => None,
        }
    } else {
        None
    };
    let latest_hash = avail.rpc.chain.get_block_hash(block_number).await.unwrap();
    if last_block_hash != Some(latest_hash) {
        println!(
            "block hash for block number {}: {:?}",
            block_number.unwrap_or(0),
            latest_hash
        );
        let context = zmq::Context::new();
        let sender = context
            .socket(zmq::REQ)
            .expect("Failed to create REQ socket");

        let zmq_socket_url =
            std::env::var("ZMQ_CHANNEL_URL").unwrap_or_else(|_| "tcp://0.0.0.0:40006".to_string());
        println!("zmq socket: {:?}", zmq_socket_url);
        sender
            .connect(&zmq_socket_url)
            .expect("failed to connect to endpoint");

        let data: Vec<Vec<u8>> = vec![
            b"datablock".to_vec(),
            format!(
                "{}-chain-{}",
                identifier,
                hex::encode(latest_hash.as_bytes())
            )
            .into_bytes(),
            b"!!!!!".to_vec(),
        ];

        sender
            .send_multipart(&data, 0)
            .expect("failed to send data");
        println!("data sent");
    }
    // let response = sender.recv_msg(0).expect("failed to receive response");
    // println!("Response received: {:?}", response);
    Ok(latest_hash)
}
