use axum::{
    routing::{get, put},
    Router, extract::Path,
    http::StatusCode,
    body::Bytes,
};
use tokio::fs;
use tracing::{error, info, warn, Level};

/* TODO:
Make it streaming and make transfer-encoding: chunked be a thing
Deal with concurrent creation of files by writing to temp and then moving the full file
It would be cool if narinfo could be tcp-nodelay and small buffer and
nars could be large buffers and sendfile. Idk if i can do that in axum or if i need
to use tower(?) hyper(?). Whatever the bottom of the stack there is.

*/

async fn get_cache_info() -> &'static str {
    info!("Serving nix-cache-info");
    "StoreDir: /nix/store\nWantMassQuery: 1\nPriority: 30"
}
async fn get_narinfo(Path(hash): Path<String>) -> StatusCode {
    info!(hash = %hash, "Fetching narinfo");
    match fs::read_to_string(format!("nar/{}.narinfo", hash)).await {
        Ok(_content) => StatusCode::OK,
        Err(_) => {
            info!(hash = %hash, "narinfo not found");  // Changed from error! to info!
            StatusCode::NOT_FOUND
        }
    }
}

async fn get_nar(Path(hash): Path<String>) -> Result<Bytes, StatusCode> {
    info!(hash = %hash, "Fetching NAR");
    match fs::read(format!("nar/{}.nar", hash)).await {
        Ok(bytes) => {
            info!(hash = %hash, size = bytes.len(), "Successfully read NAR");
            Ok(Bytes::from(bytes))
        }
        Err(_) => {
            info!(hash = %hash, "NAR not found");  // Changed from error! to info!
            Err(StatusCode::NOT_FOUND)
        }
    }
}

async fn upload_narinfo(Path(hash): Path<String>, body: String) -> StatusCode {
    info!(hash = %hash, size = body.len(), "Uploading narinfo");
    match fs::write(format!("nar/{}", hash), body).await {
        Ok(_) => {
            info!(hash = %hash, "Successfully wrote narinfo");
            StatusCode::OK
        }
        Err(e) => {
            error!(hash = %hash, error = %e, "Failed to write narinfo");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

async fn upload_nar(Path(hash): Path<String>, body: Bytes) -> StatusCode {
    warn!(hash = %hash, size = body.len(), "Uploading NAR");

    match fs::write(format!("nar/{}", hash), body).await {
        Ok(_) => {
            info!(hash = %hash, "Successfully wrote NAR");
            StatusCode::OK
        }
        Err(e) => {
            error!(hash = %hash, error = %e, "Failed to write NAR");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Starting Nix cache server");

    let app = Router::new()
        .route("/nix-cache-info", get(get_cache_info))
        .route("/:hash.narinfo", get(get_narinfo))
        .route("/:hash.narinfo", put(upload_narinfo))
        .route("/nar/:hash.nar", get(get_nar))
        .route("/nar/:hash.nar", put(upload_nar));

    let addr = "0.0.0.0:3000";
    info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
