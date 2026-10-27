use anyhow::{Context, Result};
use clap::Parser;
use mzk::{
    cli::{Cli, Command},
    player::{Player, executable},
    provider::Jamendo,
};
use std::time::Instant;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(error) = run(cli).await {
        eprintln!("mzk: {error:#}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<()> {
    let began = Instant::now();
    let Command::P { text } = cli.command;
    let id = std::env::var("JAMENDO_CLIENT_ID").context(
        "set JAMENDO_CLIENT_ID to your application's client ID from https://devportal.jamendo.com/",
    )?;
    let provider = Jamendo::new(id)?;
    let mpv = executable()?;
    let search = Instant::now();
    let (track, requests) = provider.select(&text, &mut rand::rng()).await?;
    let search_ms = search.elapsed().as_millis();
    let playback = Player::new(mpv)?.play(&track.audio).await?;
    println!("{}", track.attribution());
    if std::env::var("MZK_DIAGNOSTICS").as_deref() == Ok("1") {
        eprintln!(
            "search_ms={search_ms} resolve_ms=0 launch_ms={} confirm_ms={} time_to_playing_ms={} api_requests={requests} reused={}",
            playback.launch_ms,
            playback.confirm_ms,
            began.elapsed().as_millis(),
            playback.reused
        );
    }
    Ok(())
}
