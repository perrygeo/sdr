use std::collections::BTreeMap;
use std::time::Duration;

use clap::Parser;
use rs1090::decode::bds::bds09::AirborneVelocitySubType;
use rs1090::decode::bds::bds61::EmergencyState;
use rs1090::decode::cpr::{self, AircraftState};
use rs1090::decode::{Capability, FlightStatus};
use rs1090::prelude::*;
use tokio::net::TcpStream;

/// Read a Beast binary feed and decode Mode-S / ADS-B messages.
#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Beast feed host:port
    #[arg(long, default_value = "x1:30005")]
    host: String,
}

/// Basic aircraft information extracted from a decoded Mode-S message.
#[derive(Debug)]
struct AdsbMessage {
    #[allow(dead_code)] // shown via derived Debug
    icao24: ICAO,
    callsign: Option<String>,
    altitude_ft: Option<i32>,
    squawk: Option<u16>,
    speed_kt: Option<f64>,
    heading_deg: Option<f64>,
    vertical_rate_fpm: Option<i16>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    rssi_db: Option<f32>,
    on_ground: Option<bool>,
    // altitude_source: Option<String>,
    emergency: Option<String>,
    wake_vortex: Option<String>,
    // speed_type: Option<String>,
    // nuc_p: Option<u8>,
    // nac_v: Option<u8>,
    // sil: Option<u8>,
    // version: Option<String>,
    pressure_setting_mbar: Option<f32>,
    selected_altitude_ft: Option<u16>,
    selected_heading_deg: Option<f32>,
}

/// CPR decoding state: pairs even/odd position messages per aircraft.
#[derive(Default)]
struct CprState {
    aircraft: BTreeMap<ICAO, AircraftState>,
    reference: Option<Position>,
}

impl AdsbMessage {
    fn empty(icao24: ICAO) -> Self {
        AdsbMessage {
            icao24,
            callsign: None,
            altitude_ft: None,
            squawk: None,
            speed_kt: None,
            heading_deg: None,
            vertical_rate_fpm: None,
            latitude: None,
            longitude: None,
            rssi_db: None,
            on_ground: None,
            // altitude_source: None,
            emergency: None,
            wake_vortex: None,
            // speed_type: None,
            // nuc_p: None,
            // nac_v: None,
            // sil: None,
            // version: None,
            pressure_setting_mbar: None,
            selected_altitude_ft: None,
            selected_heading_deg: None,
        }
    }

    /// Decode a 7- or 14-byte Mode-S frame into basic aircraft information.
    fn from_frame(
        frame: &[u8],
        timestamp: f64,
        rssi: Option<f32>,
        cpr: &mut CprState,
    ) -> Option<Self> {
        let msg = Message::try_from(frame).ok()?;

        let out = match msg.df {
            ExtendedSquitterADSB(mut adsb) => {
                // Pair even/odd CPR messages to decode position.
                cpr::decode_position(
                    &mut adsb.message,
                    timestamp,
                    &adsb.icao24,
                    &mut cpr.aircraft,
                    &mut cpr.reference,
                    &None,
                );
                let mut out = AdsbMessage::empty(adsb.icao24);
                out.on_ground = match adsb.capability {
                    Capability::AG_GROUND => Some(true),
                    Capability::AG_AIRBORNE => Some(false),
                    _ => None,
                };
                match adsb.message {
                    ME::BDS08 { inner, .. } => {
                        out.callsign = Some(inner.callsign.trim().to_string());
                        out.wake_vortex = Some(format!("{}", inner.wake_vortex));
                    }
                    ME::BDS05 { inner, .. } => {
                        out.altitude_ft = inner.alt;
                        out.latitude = inner.latitude;
                        out.longitude = inner.longitude;
                        // out.altitude_source = Some(format!("{}", inner.source));
                        // out.nuc_p = Some(inner.nuc_p);
                    }
                    ME::BDS06 { inner, .. } => {
                        out.latitude = inner.latitude;
                        out.longitude = inner.longitude;
                        out.speed_kt = inner.groundspeed;
                        out.heading_deg = inner.track;
                    }
                    ME::BDS09(vel) => {
                        match vel.velocity {
                            AirborneVelocitySubType::GroundSpeedDecoding(g) => {
                                out.speed_kt = Some(g.ew_vel.hypot(g.ns_vel));
                            }
                            AirborneVelocitySubType::AirspeedSubsonic(a) => {
                                out.speed_kt = a.airspeed.map(f64::from);
                                out.heading_deg = a.heading;
                                // out.speed_type = Some(format!("{}", a.airspeed_type));
                            }
                            AirborneVelocitySubType::AirspeedSupersonic(a) => {
                                out.speed_kt = a.airspeed.map(f64::from);
                                out.heading_deg = a.heading.map(f64::from);
                                // out.speed_type = Some(format!("{}", a.airspeed_type));
                            }
                            _ => {}
                        }
                        out.vertical_rate_fpm = vel.vertical_rate;
                        // out.nac_v = Some(vel.nac_v);
                    }
                    ME::BDS61(status) => {
                        out.squawk = Some(status.squawk.0);
                        if status.emergency_state != EmergencyState::None {
                            out.emergency = Some(format!("{}", status.emergency_state));
                        }
                    }
                    ME::BDS62(status) => {
                        out.pressure_setting_mbar = status.barometric_setting;
                        out.selected_altitude_ft = status.selected_altitude;
                        out.selected_heading_deg = status.selected_heading;
                        // out.sil = Some(status.sil);
                    }
                    // ME::BDS65(status) => {
                    //     out.version = Some(match status {
                    //         AircraftOperationStatus::Airborne(op) => version_line(&op.version),
                    //         AircraftOperationStatus::Surface(op) => version_line(&op.version),
                    //         _ => format!("{status:?}"),
                    //     });
                    // }
                    _ => {}
                }
                Some(out)
            }
            SurveillanceAltitudeReply { ac, ap, fs, .. } => {
                let mut out = AdsbMessage::empty(ICAO(ap.0));
                out.altitude_ft = ac.0;
                out.on_ground = on_ground(fs);
                Some(out)
            }
            CommBAltitudeReply { ac, ap, fs, .. } => {
                let mut out = AdsbMessage::empty(ICAO(ap.0));
                out.altitude_ft = ac.0;
                out.on_ground = on_ground(fs);
                Some(out)
            }
            SurveillanceIdentityReply { id, ap, fs, .. } => {
                let mut out = AdsbMessage::empty(ICAO(ap.0));
                out.squawk = Some(id.0);
                out.on_ground = on_ground(fs);
                Some(out)
            }
            CommBIdentityReply { id, ap, fs, .. } => {
                let mut out = AdsbMessage::empty(ICAO(ap.0));
                out.squawk = Some(id.0);
                out.on_ground = on_ground(fs);
                Some(out)
            }
            AllCallReply { icao, .. } => Some(AdsbMessage::empty(icao)),
            _ => None,
        };

        out.map(|mut msg| {
            msg.rssi_db = rssi;
            msg
        })
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let mut cpr = CprState::default();

    loop {
        match TcpStream::connect(&args.host).await {
            Ok(stream) => {
                let messages = beast::next_msg(beast::DataSource::Tcp(stream)).await;
                tokio::pin!(messages);
                while let Some(msg) = messages.next().await {
                    // Skip the 1-byte escape, 1-byte type, 6-byte MLAT
                    // timestamp and 1-byte RSSI to reach the Mode-S frame.
                    let timestamp = mlat_timestamp(&msg);
                    let rssi = beast_rssi(&msg);
                    if let Some(msg) = AdsbMessage::from_frame(&msg[9..], timestamp, rssi, &mut cpr)
                    {
                        dbg!(msg);
                    }
                }
            }
            Err(e) => eprintln!("failed to connect to {}: {e}", args.host),
        }
        // Connection lost; retry forever.
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Beast MLAT timestamp (msg[2..8]): 48-bit counter ticking at 12 MHz, as
/// emitted by readsb / Mode-S Beast classic without GPS sync. Returns
/// seconds as f64. (A GPS-synced Radarcape instead emits 18 bits of seconds
/// of day + 30 bits of nanoseconds.)
fn mlat_timestamp(msg: &[u8]) -> f64 {
    let raw = u64::from_be_bytes([0, 0, msg[2], msg[3], msg[4], msg[5], msg[6], msg[7]]);
    raw as f64 / 12e6
}

/// Signal level from the Beast header (msg[8]): 0xff means unknown.
/// Returns RSSI in dB.
fn beast_rssi(msg: &[u8]) -> Option<f32> {
    if msg[8] == 0xff {
        return None;
    }
    let v = msg[8] as f32 / 255.;
    Some(10. * (v * v).log10())
}

/// On-ground state from a DF4/5/20/21 flight status field.
fn on_ground(fs: FlightStatus) -> Option<bool> {
    match fs {
        FlightStatus::NoAlertNoSpiAirborne | FlightStatus::AlertNoSpiAirborne => Some(false),
        FlightStatus::NoAlertNoSpiOnGround | FlightStatus::AlertNoSpiOnGround => Some(true),
        // "airborne/ground" states and reserved values: ambiguous
        _ => None,
    }
}

/// First line of a BDS 6,5 version Display, e.g. "Version 2 (DO-260B)".
pub fn version_line(v: &impl std::fmt::Display) -> String {
    format!("{v}")
        .lines()
        .next()
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(frame: &[u8], timestamp: f64, cpr: &mut CprState) -> AdsbMessage {
        AdsbMessage::from_frame(frame, timestamp, None, cpr).unwrap()
    }

    #[test]
    fn decode_callsign() {
        // DF17, BDS 0,8: EZY85MH (from rs1090 test suite)
        let frame = [
            0x8d, 0x40, 0x6b, 0x90, 0x20, 0x15, 0xa6, 0x78, 0xd4, 0xd2, 0x20, 0xaa, 0x4b, 0xda,
        ];
        let msg = decode(&frame, 0.0, &mut CprState::default());
        assert_eq!(msg.icao24, ICAO(0x406b90));
        assert_eq!(msg.callsign.as_deref(), Some("EZY85MH"));
    }

    #[test]
    fn decode_altitude() {
        // DF17, BDS 0,5: airborne position, -325 ft (from rs1090 test suite)
        let frame = [
            0x8d, 0x48, 0x4f, 0xde, 0x58, 0x03, 0xb6, 0x47, 0xec, 0xec, 0x4f, 0xcd, 0xd7, 0x4f,
        ];
        let msg = decode(&frame, 0.0, &mut CprState::default());
        assert_eq!(msg.icao24, ICAO(0x484fde));
        assert_eq!(msg.altitude_ft, Some(-325));
        // Single message, no even/odd pair or reference: no position yet
        assert_eq!(msg.latitude, None);
        assert_eq!(msg.longitude, None);
    }

    #[test]
    fn decode_velocity() {
        // DF17, BDS 0,9: vertical rate +64 ft/min (from rs1090 test suite)
        let frame = [
            0x8d, 0x34, 0x61, 0xcf, 0x99, 0x08, 0x38, 0x89, 0x30, 0x08, 0x0f, 0x94, 0x8e, 0xa1,
        ];
        let msg = decode(&frame, 0.0, &mut CprState::default());
        assert_eq!(msg.icao24, ICAO(0x3461cf));
        assert_eq!(msg.vertical_rate_fpm, Some(64));
    }

    #[test]
    fn decode_target_state() {
        // DF17, BDS 6,2 (from rs1090 test suite): selected altitude 17000 ft,
        // barometric setting 1012.8 mbar
        let frame = [
            0x8d, 0xa0, 0x56, 0x29, 0xea, 0x21, 0x48, 0x5c, 0xbf, 0x3f, 0x8c, 0xad, 0xae, 0xeb,
        ];
        let msg = decode(&frame, 0.0, &mut CprState::default());
        assert_eq!(msg.icao24, ICAO(0xa05629));
        assert_eq!(msg.selected_altitude_ft, Some(17000));
        assert!((msg.pressure_setting_mbar.unwrap() - 1012.8).abs() < 0.01);
    }

    #[test]
    fn test_beast_rssi() {
        // Unknown signal level
        let mut msg = [0u8; 23];
        msg[8] = 0xff;
        assert_eq!(beast_rssi(&msg), None);
        // Half scale: 20*log10(128/255) ≈ -5.99 dB
        msg[8] = 0x80;
        assert!((beast_rssi(&msg).unwrap() + 5.99).abs() < 0.05);
    }

    #[test]
    fn decode_global_position() {
        // Even/odd airborne position pair (from rs1090 cpr tests):
        // globally decodes to 49.81755N 6.08442E
        let even = [
            0x8d, 0x40, 0x05, 0x8b, 0x58, 0xc9, 0x01, 0x37, 0x51, 0x47, 0xef, 0xd0, 0x93, 0x57,
        ];
        let odd = [
            0x8d, 0x40, 0x05, 0x8b, 0x58, 0xc9, 0x04, 0xa8, 0x7f, 0x40, 0x2d, 0x3b, 0x8c, 0x59,
        ];
        let mut cpr = CprState::default();
        let first = decode(&even, 100.0, &mut cpr);
        assert_eq!(first.latitude, None); // needs the odd frame too
        let second = decode(&odd, 100.2, &mut cpr);
        assert!((second.latitude.unwrap() - 49.81755).abs() < 0.05);
        assert!((second.longitude.unwrap() - 6.08442).abs() < 0.05);
    }
}
