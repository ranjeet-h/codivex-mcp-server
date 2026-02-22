#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ui_dioxus::server::run_ui_server().await
}
