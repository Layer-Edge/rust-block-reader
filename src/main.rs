use avail_rust::{hex, H256};
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

mod block_number_op;
mod block_reader;
mod cli_args;
mod router;
mod rpc_call;
mod util;

use block_number_op::{read_block_number, write_block_number};
use block_reader::BlockReader;
use cli_args::{Args, Mode};
use router::Router;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let args = Args::parse();

    fs::create_dir_all("block_numbers")?;

    let br = Arc::new(BlockReader::new());

        match args.mode {
        Mode::TEST => {
            let number = read_block_number("celestia");
            let updated_number = if number.is_none() {
                1
            } else {
                number.unwrap() + 1
            };
            write_block_number("celestia", updated_number)?;
        }
        Mode::REST => rest_server(br.clone()).await?,
        Mode::LOOP => iterate_block_reader(br.clone()).await?,
        Mode::BOTH => {
            if let Err(e) = tokio::try_join!(
                rest_server(br.clone()),
                iterate_block_reader(br.clone()),
            ) {
                eprintln!("Error in BOTH mode: {}", e);
            }
        }
    }

    Ok(())
}

async fn iterate_block_reader(br: Arc<BlockReader>) -> Result<()> {
    let mut last_block_hash: Option<H256> = None;
    let block_fetch_params: Vec<(&str, &str, &str, &str, Option<&str>)> = vec![
        ("sdk", "avail", "", "", None),
        (
            "rpc",
            "onlylayer",
            "https://onlylayer.org",
            "eth_getBlockByNumber",
            None,
        ),
        (
            "rpc",
            "mintchain",
            "https://global.rpc.mintchain.io",
            "eth_getBlockByNumber",
            None,
        ),
        (
            "rpc",
            "bitfinity",
            "https://mainnet.bitfinity.network",
            "eth_getBlockByNumber",
            None,
        ),
        (
            "rpc",
            "u2u",
            "https://rpc-mainnet.u2u.xyz",
            "eth_getBlockByNumber",
            None,
        ),
        (
            "rpc",
            "celestia",
            "https://celestia-archival.rpc.grove.city/v1/097ddf85",
            "header.GetByHeight",
            None,
        ),
        (
            "rpc",
            "kaanch",
            "https://rpc.kaanch.network",
            "kaanch_latestblocks",
            None,
        ),
    ];

    loop {
        for (_type, chain, rpc_url, method, auth) in block_fetch_params.clone() {
            match _type {
                "sdk" => {
                    let fetched_number = read_block_number("avail");
                    let block_number = if fetched_number.is_none() {
                        String::from("")
                    } else {
                        format!("{}", &fetched_number.unwrap())
                    };
                    match br.fetch_block_hash(
                        "avail".to_string(),
                        block_number.as_str(),
                        last_block_hash,
                    )
                    .await
                    {
                        Ok((block_hash, block_number)) => {
                            write_block_number("avail", block_number + 1)?;
                            last_block_hash = Some(block_hash);
                        }
                        Err(e) => eprintln!("Failed to fetch block hash {:?}", e),
                    }
                }
                "rpc" => {
                    br.block_hash_from_rpc(chain, rpc_url, method, auth).await?
                }
                _ => {
                    println!("unknown type call");
                }
            }
        }
        sleep(Duration::from_millis(5000)).await;
    }
}

async fn rest_server(br: Arc<BlockReader>) -> Result<()> {
    let mut router = Router::new();

    router.add_route(
        "/add-block-by-number/".to_string(),
        move |block_number: String| {
            let br_clone = br.clone();
            async move {
                match br_clone.fetch_block_hash("o3".to_string(), &block_number, None).await {
                    Ok((block_hash, _)) => {
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
