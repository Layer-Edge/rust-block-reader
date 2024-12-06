use std::{
    fs,
    io::{Error, ErrorKind, Result},
    sync::Arc,
};
use avail_rust::{hex, H256, SDK};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

mod router;

use router::Router;

#[tokio::main]
async fn main() -> Result<()> {
    let mut router = Router::new();

    router.add_route("/add-block-by-number/".to_string(), |block_number: String| async move {
        match fetch_block_hash(&block_number).await {
            Ok(block_hash) => {
                println!("received block hash {:?}", block_hash);
                format!(
                    "{{\"msg\": \"block hash added successfully\", \"block_hash\": \"0x{}\"}}",
                    hex::encode(block_hash.as_bytes())
                )
            },
            Err(e) => format!(
                "{{\"error\": \"Failed to fetch block hash\", \"details\": \"{}\"}}",
                e.to_string()
            ),
        }
    });

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
        Some(json_body) => (
            "200 OK", 
            "application/json", 
            json_body
        ),
        None => (
            "404 NOT FOUND", 
            "text/html", 
            fs::read_to_string("src/404.html").unwrap()
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

async fn fetch_block_hash(_block_number: &str) -> std::result::Result<H256, Box<dyn std::error::Error>> {
    let avail = SDK::new("wss://mainnet.avail-rpc.com/").await?;
    let block_number = if !_block_number.is_empty() {
        match _block_number.trim().parse::<u32>() {
            Ok(num) => Some(num),
            Err(_) => None,
        }
    } else {
        None
    };
    let latest_hash = avail.rpc.chain.get_block_hash(block_number).await.unwrap();
    println!("block hash for block number {}: {:?}", block_number.unwrap_or(0), latest_hash);
    let mut stream = TcpStream::connect("0.0.0.0:40002").await?;

    let now = std::time::SystemTime::now();
    let timestamp = format!("{:?}", now);
    let data: Vec<Vec<u8>> = vec![
        b"datablock".to_vec(),
        format!("Hello world!! {}", timestamp).into_bytes(),
        format!("{}", hex::encode(latest_hash.as_bytes())).into_bytes(),
    ];

    let flattened_data: Vec<u8> = data.concat();

    stream.write_all(&flattened_data).await?;
    // tokio::time::sleep(Duration::from_millis(5000)).await;
    stream.flush().await?;
    Ok(latest_hash)
}
