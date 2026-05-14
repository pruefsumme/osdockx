use anyhow::Result;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    osdockx::ui::run()
}
