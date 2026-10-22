//! The remote binary is only useful if it speaks the socket's actual protocol.
//! This test starts a loopback listener, writes the endpoint file the app would
//! write, and runs the real `shadowplayer-remote` process against it.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;

#[test]
fn remote_sends_a_token_scoped_command() {
    let dir = std::env::temp_dir().join(format!(
        "sp_remote_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::fs::write(
        dir.join("instance.json"),
        format!(r#"{{"pid":1,"port":{port},"token":"test-token"}}"#),
    )
    .unwrap();

    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        let request: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(request["token"], "test-token");
        assert_eq!(request["cmd"], "next");
        stream
            .write_all(b"{\"ok\":true,\"state\":{\"loaded\":false,\"playing\":false,\"title\":\"\",\"artist\":\"\",\"album\":\"\",\"length_secs\":0.0,\"position_secs\":0.0,\"volume\":0.5}}\n")
            .unwrap();
    });

    let status = std::process::Command::new(env!("CARGO_BIN_EXE_shadowplayer-remote"))
        .arg("next")
        .env("SHADOWPLAYER_DATA_DIR", PathBuf::from(&dir))
        .status()
        .expect("the remote binary must build and run");
    server.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(status.success(), "remote exited with {status}");
}
