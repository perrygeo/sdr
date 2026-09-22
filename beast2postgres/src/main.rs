use std::time::Duration;

use batch_copy::{Configuration, Copier};
use clap::Parser;
use rs1090::prelude::*;
use tokio::net::TcpStream;

mod adbs;
mod util;

use adbs::{AdsbMessage, CprState, beast_rssi, mlat_timestamp};
use util::format_status;

const LOG_EVERY: f64 = 1.0; // seconds

/// Read a Beast binary feed and decode Mode-S / ADS-B messages.
#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Beast protocol host:port
    #[arg(long, default_value = "localhost:30005")]
    host: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let mut cpr = CprState::default();

    eprintln!("Connecting to Database...");
    let url = std::env::var("DATABASE_URL").unwrap();
    let copy_cfg = Configuration::new()
        .database_url(url)
        // Flush at least this often (milliseconds)
        .flush_timer_ms(2000)
        // Or when this many rows have accumulated
        .max_rows_per_batch(2000)
        // Channel capacity before backpressure kicks in
        .max_channel_capacity(20_000)
        .build();

    let copier = Copier::<AdsbMessage>::new(copy_cfg).await.unwrap();

    loop {
        eprintln!("Listening for ADS-B messages...");
        match TcpStream::connect(&args.host).await {
            Ok(stream) => {
                let messages = beast::next_msg(beast::DataSource::Tcp(stream)).await;
                tokio::pin!(messages);
                let mut seen = 0u64;
                let started = tokio::time::Instant::now();
                let mut last_log = started;
                while let Some(msg) = messages.next().await {
                    // Skip the 1-byte escape, 1-byte type, 6-byte MLAT
                    // timestamp and 1-byte RSSI to reach the Mode-S frame.
                    let timestamp = mlat_timestamp(&msg);
                    let rssi = beast_rssi(&msg);
                    if let Some(msg) = AdsbMessage::from_frame(&msg[9..], timestamp, rssi, &mut cpr)
                    {
                        // dbg!(msg);
                        copier.send(msg).await;
                        seen += 1;
                    }
                    let elapsed = last_log.elapsed().as_secs_f64();
                    if elapsed >= LOG_EVERY {
                        print!("\r\x1b[2K{}", format_status(seen, started.elapsed()));
                        std::io::Write::flush(&mut std::io::stdout()).unwrap();
                        last_log = tokio::time::Instant::now();
                    }
                }
                eprintln!();
            }
            Err(e) => eprintln!("failed to connect to {}: {e}", args.host),
        }
        copier.flush().await;

        // Connection lost; retry forever.
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
