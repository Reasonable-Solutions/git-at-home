use axum::{
    body::{BodyDataStream, Bytes},
    extract::Path,
    http::StatusCode,
    routing::{get, put},
    Router,
};

use futures::StreamExt;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio_util;
use tracing::{error, info, warn, Level};
use uuid::{self, Uuid};

// TODO: What is a sensible priority value? why does the cache hav it?
async fn get_cache_info() -> &'static str {
    info!("Serving nix-cache-info");
    "StoreDir: /nix/store\nWantMassQuery: 1\nPriority: 20"
}

// These could just be generated on the fly. A narinfo is just a few lines describing a
// narfile
async fn disk_get_narinfo(Path(hash): Path<String>) -> Result<String, StatusCode> {
    info!(hash = %hash, "Fetching narinfo");
    match fs::read_to_string(format!("nar/{}", hash)).await {
        Ok(content) => Ok(content),
        Err(_) => {
            info!(hash = %hash, "narinfo not found");
            Err(StatusCode::NOT_FOUND)
        }
    }
}

// TODO: this should be streaming
async fn disk_get_nar(
    Path(hash): Path<String>,
) -> Result<impl axum::response::IntoResponse, StatusCode> {
    info!(hash = %hash, "Fetching NAR");
    match fs::read(format!("nar/{}", hash)).await {
        Ok(bytes) => {
            info!(hash = %hash, size = bytes.len(), "Successfully read NAR");
            Ok(Bytes::from(bytes))
        }
        Err(_) => {
            info!(hash = %hash, "NAR not found");
            Err(StatusCode::NOT_FOUND)
        }
    }
}

async fn disk_put_narinfo(Path(hash): Path<String>, body: String) -> StatusCode {
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

async fn disk_put_nar(
    Path(hash): Path<String>,
    body: axum::body::Body,
) -> Result<StatusCode, StatusCode> {
    warn!(hash = %hash, "Starting NAR upload");

    let cache_dir = "nar"; // this is config
    let temp_path = format!("{}/{}.{}.temp", cache_dir, hash, Uuid::new_v4());
    let final_path = format!("{}/{}", cache_dir, hash);

    // We create a temporary file and if everything goes well we yeet that into the
    // cache.
    let mut file = match tokio::fs::File::create(&temp_path).await {
        Ok(file) => file,
        Err(e) => {
            error!(hash = %hash, error = %e, "Failed to create temp file");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let mut stream = body.into_data_stream();
    while let Some(chunk_result) = stream.next().await {
        let chunk: Bytes = match chunk_result {
            Ok(chunk) => chunk,
            Err(e) => {
                error!(hash = %hash, error = %e, "Failed to read chunk");
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        };

        if let Err(e) = file.write_all(&chunk).await {
            error!(hash = %hash, error = %e, "Failed to write chunk");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    if let Err(e) = tokio::fs::rename(temp_path, final_path).await {
        error!(hash = %hash, error = %e, "Failed to rename temp file");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    info!(hash = %hash, "Successfully wrote NAR");
    Ok(StatusCode::OK)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();
    let cache_dir = "nar";
    tokio::fs::create_dir_all(cache_dir)
        .await
        .expect("failed to create cache dir");
    info!("Starting Nix cache server");

    let app = Router::new()
        .route("/nix-cache-info", get(get_cache_info))
        .route("/:hash.narinfo", get(disk_get_narinfo))
        .route("/:hash.narinfo", put(disk_put_narinfo))
        .route("/nar/:hash.nar", get(disk_get_nar))
        .route("/nar/:hash.nar", put(disk_put_nar));

    let addr = "0.0.0.0:3000";
    info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

trait NixCacheStorage {
    async fn get_narinfo(&self, hash: &str) -> Result<String, StatusCode>;
    async fn put_narinfo(&self, hash: &str, content: String) -> Result<(), StatusCode>;
    async fn get_nar(&self, hash: &str) -> Result<Bytes, StatusCode>;
    async fn put_nar(&self, hash: &str, content: axum::body::Body) -> Result<(), StatusCode>;
}

// For hetzner buckets
struct S3Storage {
    bucket: String,
    base_dir: String,
    credential: (String, String),
}

use axum::body::Body;
use bytes::Bytes;
use s3::{creds::Credentials, request::ResponseData, Bucket, Region};
use std::io::Cursor;
use tokio::io::AsyncReadExt;

impl NixCacheStorage for S3Storage {
    async fn get_narinfo(&self, hash: &str) -> Result<String, StatusCode> {
        let region = Region::Custom {
            region: "eu-central-1".to_owned(), // Adjust for Hetzner
            endpoint: "https://s3.eu-central-1.amazonaws.com".to_owned(), // Adjust endpoint for Hetzner
        };

        let credentials = Credentials::new(
            Some(&self.credential.0),
            Some(&self.credential.1),
            None,
            None,
            None,
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let bucket = Bucket::new(&self.bucket, region, credentials)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let path = format!("{}/{}.narinfo", self.base_dir, hash);

        let (data, _) = bucket
            .get_object(&path)
            .await
            .map_err(|_| StatusCode::NOT_FOUND)?;

        String::from_utf8(data).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    }

    async fn put_narinfo(&self, hash: &str, content: String) -> Result<(), StatusCode> {
        let region = Region::Custom {
            region: "eu-central-1".to_owned(),
            endpoint: "https://s3.eu-central-1.amazonaws.com".to_owned(),
        };

        let credentials = Credentials::new(
            Some(&self.credential.0),
            Some(&self.credential.1),
            None,
            None,
            None,
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let bucket = Bucket::new(&self.bucket, region, credentials)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let path = format!("{}/{}.narinfo", self.base_dir, hash);

        bucket
            .put_object(&path, content.as_bytes())
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        Ok(())
    }

    async fn get_nar(&self, hash: &str) -> Result<Bytes, StatusCode> {
        let region = Region::Custom {
            region: "eu-central-1".to_owned(),
            endpoint: "https://s3.eu-central-1.amazonaws.com".to_owned(),
        };

        let credentials = Credentials::new(
            Some(&self.credential.0),
            Some(&self.credential.1),
            None,
            None,
            None,
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let bucket = Bucket::new(&self.bucket, region, credentials)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let path = format!("{}/nar/{}.nar", self.base_dir, hash);

        let ResponseData(data, _) = bucket
            .get_object(&path)
            .await
            .map_err(|_| StatusCode::NOT_FOUND)?;

        Ok(Bytes::from(data))
    }

    async fn put_nar(&self, hash: &str, content: Body) -> Result<(), StatusCode> {
        let region = Region::Custom {
            region: "eu-central-1".to_owned(),
            endpoint: "https://s3.eu-central-1.amazonaws.com".to_owned(),
        };

        let credentials = Credentials::new(
            Some(&self.credential.0),
            Some(&self.credential.1),
            None,
            None,
            None,
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let bucket = Bucket::new(&self.bucket, region, credentials)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let path = format!("{}/{}.narinfo", self.base_dir, hash);

        bucket
            .put_object(&path, content.as_bytes())
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        Ok(())
    }
}
