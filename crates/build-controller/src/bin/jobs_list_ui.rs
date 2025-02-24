use axum::{routing::get, Router};
use build_controller::NixBuild;
use kube::{Api, Client};
use tracing::info;
use tracing::Level;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting job-list-ui");

    let app = Router::new().route("/jobs-list", get(list_jobs));

    let addr = "0.0.0.0:3000";
    info!("Listening on {}", &addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    axum::serve(listener, app).await.unwrap();
}

async fn list_jobs() -> axum::response::Html<String> {
    let client = Client::try_default().await.unwrap();
    let builds: Api<NixBuild> = Api::namespaced(client, "default");

    let builds = builds
        .list(&kube::api::ListParams::default())
        .await
        .unwrap();

    let builds_html = builds
        .items
        .iter()
        .map(|build| {
            format!(
                "<tr>
                    <td>{}</td>
                    <td>{}</td>
                    <td>{}</td>
                </tr>",
                build.metadata.name.clone().unwrap_or_default(),
                build
                    .status
                    .as_ref()
                    .map_or("Unknown".to_string(), |s| s.phase.clone()),
                build.spec.image_name
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    axum::response::Html(format!(
        r#"<!DOCTYPE html>
        <html>
        <head>
            <title>Nix Builds</title>
            <script src="https://unpkg.com/htmx.org@1.9.6"></script>
            <style>
                table {{ border-collapse: collapse; width: 100%; }}
                th, td {{ border: 1px solid #ddd; padding: 8px; text-align: left; }}
                th {{ background-color: #f2f2f2; }}
            </style>
        </head>
        <body>
            <h1>Nix Builds</h1>
            <div hx-get="/jobs-list" hx-trigger="every 5s">
                <table>
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
                </table>
            </div>
        </body>
        </html>"#
    ))
}
