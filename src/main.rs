use crate::extractor::extractor::{run_extractor, DATA_DIR, EPUBS_DIR};
use crate::server::auth;

mod extractor;
mod server;
mod util;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tokio::fs::create_dir_all(DATA_DIR).await?;
    tokio::fs::create_dir_all(format!("{DATA_DIR}/users")).await?;
    tokio::fs::create_dir_all(format!("{EPUBS_DIR}/private")).await?;

    match auth::bootstrap_admin().await? {
        Some(otp) => {
            println!("===========================================");
            println!("  Default admin account created.");
            println!("  Username: admin");
            println!("  Password: {otp}");
            println!("  (you will be asked to change it on login)");
            println!("===========================================");
        }
        None => {
            println!("Users file found, skipping admin bootstrap.");
        }
    }

    auth::backfill_owners().await?;

    let extractor = tokio::spawn(run_extractor());
    let server = tokio::spawn(run_server());
    tokio::select! {
        res = extractor => res??,
        res = server => res??,
    }
    Ok(())
}

async fn run_server() -> anyhow::Result<()> {
    let app = server::server::router();
    let listener = tokio::net::TcpListener::bind("0.0.0.0:6969").await?;
    println!("Serving on http://0.0.0.0:6969");
    axum::serve(listener, app).await?;
    Ok(())
}
