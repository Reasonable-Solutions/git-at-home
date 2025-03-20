use axum::{routing::get, Router, routing::delete};
use build_controller::NixBuild;
use kube::{api::DeleteParams, Api, Client};
use tracing::{info, Level};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();
    info!("Starting job-list-ui");

    let app = Router::new()
        .route("/", get(index_page))
        .route("/jobs-list", get(list_jobs))
        .route("/:name", delete(delete_job));


    let addr = "0.0.0.0:3000";
    info!("Listening on {}", &addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn index_page() -> axum::response::Html<String> {
    axum::response::Html(
        r#"<!DOCTYPE html>
        <html>
        <head>
            <title>Nix Builds</title>
            <script src="https://unpkg.com/htmx.org@1.9.6"></script>


            <style>
              body {
                font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen-Sans, Ubuntu, sans-serif;
                max-width: 1200px;
                margin: 0 auto;
                padding: 20px;
                background: #f5f5f5;
              }

              table {
                width: 100%;
                border-collapse: collapse;
                background: white;
                border-radius: 8px;
                box-shadow: 0 1px 3px rgba(0,0,0,0.1);
              }

              th, td {
                padding: 12px;
                text-align: left;
                border-bottom: 1px solid #eee;
              }

              th {
                background: #f8f9fa;
                font-weight: 600;
              }

              a {
                color: #0066cc;
                text-decoration: none;
              }

              h1 {
                color: #333;
                margin-bottom: 24px;
              }
            </style>
        </head>
        <body>
            <h1>Nix Builds</h1>
            <div hx-get="/jobs-list" hx-trigger="load, every 5s">
            </div>
        </body>
        </html>"#.to_string()
    )
}

async fn list_jobs() -> axum::response::Html<String> {
    let client = Client::try_default().await.unwrap();
    let builds: Api<NixBuild> = Api::namespaced(client, "default");

    let builds = builds.list(&Default::default()).await.unwrap();

    let builds_html = builds.items.iter()
        .map(|build| format!(
            "<tr><td><button hx-delete=/jobs-list/{}>delete</button><a hx-boost=\"true\" href=/jobs/{}>{}</a></td><td>{}</td><td>{}</td></tr>",
            build.metadata.name.clone().unwrap_or_default(),
            build.metadata.name.clone().unwrap_or_default(),
            build.metadata.name.clone().unwrap_or_default(),
            build.status.as_ref().map_or("Unknown".to_string(), |s| s.phase.clone()),
            build.spec.image_name
        ))
        .collect::<Vec<_>>()
        .join("\n");

    axum::response::Html(format!(
        r#"<table>
            <thead>
                <tr>
                    <th>Name</th>
                    <th>Status</th>
                    <th>Image</th>
                </tr>
            </thead>
            <tbody>
                {builds_html}
            </tbody>
        </table>"#
    ))
}

async  fn delete_job(job: String) -> axum::response::Html<String> {
    info!("deletu, {}", &job);
    let client = Client::try_default().await.unwrap();
    let builds: Api<NixBuild> = Api::namespaced(client, "default");
    builds.delete(&job, &DeleteParams::foreground()).await.unwrap();
    let builds = builds.list(&Default::default()).await.unwrap();

    let builds_html = builds.items.iter()
        .map(|build| format!(
            "<tr><td><a hx-boost=\"true\" href=/jobs/{}>{}</a></td><td>{}</td><td>{}</td></tr>",
            build.metadata.name.clone().unwrap_or_default(),
            build.metadata.name.clone().unwrap_or_default(),
            build.status.as_ref().map_or("Unknown".to_string(), |s| s.phase.clone()),
            build.spec.image_name
        ))
        .collect::<Vec<_>>()
        .join("\n");

    axum::response::Html(format!(
        r#"<table>
            <thead>
                <tr>
                    <th>Name</th>
                    <th>Status</th>
                    <th>Image</th>
                </tr>
            </thead>
            <tbody>
                {builds_html}
            </tbody>
        </table>"#
    ))
}
