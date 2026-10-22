//! `shadowplayer-remote` — control the running player over its loopback
//! socket.
//!
//! This binary never opens the library, the audio device, or a network port.
//! It reads `<data_dir>/instance.json`, sends one JSON line to the owner and
//! exits. The endpoint is always `127.0.0.1`.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{json, Value};

fn data_dir() -> PathBuf {
    std::env::var_os("SHADOWPLAYER_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::data_local_dir().map(|d| d.join("shadowplayer")))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn usage() -> ! {
    eprintln!(
        "shadowplayer-remote — control the running ShadowPlayer\n\n\
         Usage: shadowplayer-remote <command> [value]\n\n\
         Commands:\n\
           play | pause | toggle | stop\n\
           next | prev\n\
           seek <seconds>       relative seek, e.g. seek -10\n\
           seek-to <seconds>    absolute position\n\
           volume <±percent>    relative volume, e.g. volume +5\n\
           raise                bring the window forward\n\
           state                print the current transport state\n\
           quit                 ask the owner to exit\n"
    );
    std::process::exit(2);
}

fn parse_command() -> (String, Option<f64>) {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        usage();
    };
    let value = args.next().map(|raw| match raw.parse::<f64>() {
        Ok(v) => v,
        Err(_) => {
            eprintln!("shadowplayer-remote: expected a number, got {raw:?}");
            std::process::exit(2);
        }
    });
    let mapped: &str = match command.as_str() {
        "play" => "play",
        "pause" => "pause",
        "toggle" => "toggle",
        "stop" => "stop",
        "next" => "next",
        "prev" | "previous" => "previous",
        "seek" => "seek",
        "seek-to" | "seek_to" => "seek_to",
        "volume" => "volume",
        "raise" => "raise",
        "state" => "state",
        "quit" => "quit",
        other => {
            eprintln!("shadowplayer-remote: unknown command {other:?}");
            usage();
        }
    };
    (mapped.to_string(), value)
}

fn main() {
    let (command, value) = parse_command();
    let endpoint_path = data_dir().join("instance.json");
    let raw = match std::fs::read_to_string(&endpoint_path) {
        Ok(raw) => raw,
        Err(e) => {
            eprintln!(
                "shadowplayer-remote: no running instance at {}: {e}",
                endpoint_path.display()
            );
            std::process::exit(1);
        }
    };
    let endpoint: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "shadowplayer-remote: unreadable endpoint {}: {e}",
                endpoint_path.display()
            );
            std::process::exit(1);
        }
    };
    let Some(port) = endpoint.get("port").and_then(Value::as_u64) else {
        eprintln!("shadowplayer-remote: endpoint has no port");
        std::process::exit(1);
    };
    let Some(token) = endpoint.get("token").and_then(Value::as_str) else {
        eprintln!("shadowplayer-remote: endpoint has no token");
        std::process::exit(1);
    };

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port as u16));
    let mut stream = match TcpStream::connect_timeout(&addr, Duration::from_millis(500)) {
        Ok(stream) => stream,
        Err(e) => {
            eprintln!("shadowplayer-remote: cannot reach 127.0.0.1:{port}: {e}");
            std::process::exit(1);
        }
    };
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(2))).ok();

    let mut request = json!({ "token": token, "cmd": &command });
    if let Some(value) = value {
        request["value"] = json!(value);
    }
    let mut line = serde_json::to_vec(&request).expect("request serializes");
    line.push(b'\n');
    if let Err(e) = stream.write_all(&line) {
        eprintln!("shadowplayer-remote: send failed: {e}");
        std::process::exit(1);
    }

    let mut response = String::new();
    if let Err(e) = BufReader::new(stream).read_line(&mut response) {
        eprintln!("shadowplayer-remote: no response: {e}");
        std::process::exit(1);
    }
    let response: Value = match serde_json::from_str(&response) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("shadowplayer-remote: unreadable response: {e}");
            std::process::exit(1);
        }
    };
    if response.get("ok").and_then(Value::as_bool) != Some(true) {
        let message = response
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("command rejected");
        eprintln!("shadowplayer-remote: {message}");
        std::process::exit(1);
    }

    if command == "state" {
        if let Some(state) = response.get("state") {
            let title = state
                .get("title")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("Nothing playing");
            let artist = state.get("artist").and_then(Value::as_str).unwrap_or("");
            let playing = state
                .get("playing")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let position = state
                .get("position_secs")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let volume = state.get("volume").and_then(Value::as_f64).unwrap_or(0.0);
            println!(
                "{} — {}  [{}]  {:.1}s  vol {:.0}%",
                if artist.is_empty() { title } else { artist },
                if artist.is_empty() { "" } else { title },
                if playing { "playing" } else { "paused" },
                position,
                volume * 100.0
            );
        }
    }
}
