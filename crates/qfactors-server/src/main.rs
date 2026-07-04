use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;
use qfactors_server::{DataDir, router};

/// Serve an interactive factor-evaluation report from a saved output dir.
#[derive(Parser)]
#[command(name = "qfactors-server", version)]
struct Args {
    /// Evaluation output directory (from `evaluate(output_dir=...)` / `save()`).
    #[arg(long)]
    dir: PathBuf,
    /// Port to listen on.
    #[arg(long, default_value_t = 8080)]
    port: u16,
    /// Built frontend to serve (defaults to ./frontend/dist if present).
    #[arg(long)]
    assets: Option<PathBuf>,
    /// Open the report in the default browser once the server is up.
    #[arg(long)]
    open: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let dir = DataDir::new(&args.dir)?;
    let assets = args
        .assets
        .or_else(|| Some(PathBuf::from("frontend/dist")))
        .filter(|p| p.join("index.html").is_file());

    let app = router(dir, assets);
    let addr = SocketAddr::from(([127, 0, 0, 1], args.port));
    let url = format!("http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("qfactors-server: {url}  (serving {})", args.dir.display());

    if args.open {
        open_browser(&url);
    }
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await?;
    Ok(())
}

/// Best-effort launch of the default browser; failures are non-fatal.
fn open_browser(url: &str) {
    #[cfg(target_os = "windows")]
    let cmd = ("cmd", vec!["/C", "start", "", url]);
    #[cfg(target_os = "macos")]
    let cmd = ("open", vec![url]);
    #[cfg(all(unix, not(target_os = "macos")))]
    let cmd = ("xdg-open", vec![url]);

    let _ = std::process::Command::new(cmd.0).args(cmd.1).spawn();
}
