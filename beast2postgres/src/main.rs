use std::time::Duration;

use batch_copy::{Configuration, Copier};
use clap::Parser;
use rs1090::prelude::*;
use tokio::net::TcpStream;

mod adbs;

use adbs::{AdsbMessage, CprState, beast_rssi, mlat_timestamp};

/// Read a Beast binary feed and decode Mode-S / ADS-B messages.
#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Beast protocol host:port
    #[arg(long, default_value = "localhost:30005")]
    host: String,
}

const LOG_EVERY: f64 = 1.0; // seconds

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    let mut out = String::new();
    if h > 0 {
        out.push_str(&format!("{h}h"));
    }
    if m > 0 || h > 0 {
        out.push_str(&format!("{m}min"));
    }
    out.push_str(&format!("{s}s"));
    out
}

fn format_status(msgs: u64, elapsed: Duration) -> String {
    let secs = elapsed.as_secs_f64();
    let rate = if secs > 0.0 {
        (msgs as f64 / secs).round() as u64
    } else {
        0
    };
    format!(
        "Messages: {msgs}, Time: {}, Rate: {rate}msg/s",
        format_duration(elapsed)
    )
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let mut cpr = CprState::default();

    println!("Connecting to Database...");
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
        println!("Listening for ADS-B messages...");
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
                println!();
            }
            Err(e) => eprintln!("failed to connect to {}: {e}", args.host),
        }
        copier.flush().await;

        // Connection lost; retry forever.
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_under_a_minute() {
        assert_eq!(format_duration(Duration::from_secs(45)), "45s");
    }

    #[test]
    fn duration_minutes_and_seconds() {
        assert_eq!(format_duration(Duration::from_secs(90)), "1min30s");
    }

    #[test]
    fn duration_hours() {
        assert_eq!(format_duration(Duration::from_secs(3600)), "1h0min0s");
    }

    #[test]
    fn status_with_rate() {
        assert_eq!(
            format_status(1000, Duration::from_secs(100)),
            "Messages: 1000, Time: 1min40s, Rate: 10msg/s"
        );
    }

    #[test]
    fn status_zero_elapsed_has_zero_rate() {
        assert_eq!(
            format_status(0, Duration::from_secs(0)),
            "Messages: 0, Time: 0s, Rate: 0msg/s"
        );
    }
}
