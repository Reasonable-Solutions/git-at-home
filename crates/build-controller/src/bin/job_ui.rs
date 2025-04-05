use axum::extract::Path;
use axum::{routing::get, Router};
use build_controller::NixBuild;
use kube::{Api, Client};
use tracing::{error, info, warn, Level};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting job-list-ui");

    let app = Router::new().route("/jobs/:name", get(get_job));

    let addr = "0.0.0.0:3000";
    info!("Listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn get_job(Path(name): Path<String>) -> axum::response::Html<String> {
    let client = Client::try_default().await.unwrap();
    let builds: Api<NixBuild> = Api::namespaced(client, "nixbuilder");

    let build = builds.get(&name).await.unwrap();

    axum::response::Html(format!(
        "<div>
            <h1>Build: {}</h1>
            <p>Phase: {}</p>
            <p>Image: {}</p>
            <p>Git Repo: {}</p>
            <p>Git Ref: {}</p>
        </div>",
        build.metadata.name.unwrap_or_default(),
        build.status.map_or("Unknown".to_string(), |s| s.phase),
        build.spec.image_name,
        build.spec.git_repo,
        build.spec.git_ref.unwrap_or_default()
    ))
}
