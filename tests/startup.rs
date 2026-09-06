use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Server(Child, std::path::PathBuf);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
        let _ = std::fs::remove_dir_all(&self.1);
    }
}

#[test]
fn port_serves_api_and_ignores_legacy_bind_address() {
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = reservation.local_addr().unwrap().port();
    let dir = std::env::temp_dir().join(format!("fivech-port-{}-{port}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    drop(reservation);
    let child = Command::new(env!("CARGO_BIN_EXE_viewer-of-5ch"))
        .current_dir(&dir)
        .env("PORT", port.to_string())
        .env("BIND_ADDRESS", "invalid legacy address")
        .env("DATABASE_PATH", dir.join("test.db"))
        .env("IMAGE_CACHE_DIR", dir.join("images"))
        .env("COOKIES_PATH", dir.join("cookies.json"))
        .env_remove("BASE_PATH")
        .env_remove("FIVECH_BASE_URL")
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let mut server = Server(child, dir);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            server.0.try_wait().unwrap().is_none(),
            "server exited before PORT responded"
        );
        if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            stream
                .write_all(b"GET /api/favorites HTTP/1.0\r\nHost: localhost\r\n\r\n")
                .unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            assert!(
                response.starts_with("HTTP/1.0 200") || response.starts_with("HTTP/1.1 200"),
                "{response}"
            );
            assert_eq!(response.split("\r\n\r\n").nth(1).unwrap(), "[]");
            break;
        }
        assert!(Instant::now() < deadline, "PORT did not respond");
        std::thread::sleep(Duration::from_millis(30));
    }
}
