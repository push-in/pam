use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SIGINT: i32 = 2;

unsafe extern "C" {
    fn kill(process_id: i32, signal: i32) -> i32;
}

struct DevProcess {
    child: Child,
    directory: PathBuf,
    port: u16,
}

impl DevProcess {
    fn start() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("pam-dev-test-{}-{unique}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("index.php"), php_source("before-reload")).unwrap();

        let port_probe = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = port_probe.local_addr().unwrap().port();
        drop(port_probe);

        let child = Command::new(env!("CARGO_BIN_EXE_pam"))
            .arg("dev")
            .arg("index.php")
            .current_dir(&directory)
            .env("PAM_TEST_PORT", port.to_string())
            .env("PAM_DEV_EVENTS", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Pam dev should start");

        Self {
            child,
            directory,
            port,
        }
    }

    fn overwrite_script(&self, version: &str) {
        fs::write(self.directory.join("index.php"), php_source(version)).unwrap();
    }

    fn wait_for_response(&mut self, expected: &str) {
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            if let Ok(response) = http_get(self.port)
                && response.contains(expected)
            {
                return;
            }
            if let Some(status) = self.child.try_wait().unwrap() {
                panic!("Pam dev stopped unexpectedly with {status}");
            }
            thread::sleep(Duration::from_millis(40));
        }

        panic!("Pam dev did not serve response containing {expected:?}");
    }

    fn stop(&mut self) {
        if self.child.try_wait().unwrap().is_some() {
            return;
        }

        // SAFETY: This PID belongs to the development supervisor created above.
        unsafe {
            kill(self.child.id() as i32, SIGINT);
        }

        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if self.child.try_wait().unwrap().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(25));
        }

        panic!("Pam dev did not stop after SIGINT");
    }

    fn events(&mut self) -> Vec<serde_json::Value> {
        let mut output = String::new();
        self.child
            .stderr
            .take()
            .expect("development stderr should be captured")
            .read_to_string(&mut output)
            .unwrap();
        output
            .lines()
            .filter_map(|line| line.strip_prefix("@pam-event "))
            .map(|line| serde_json::from_str(line).expect("event should be valid JSON"))
            .collect()
    }
}

impl Drop for DevProcess {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn reloads_the_php_process_after_a_file_change() {
    let mut dev = DevProcess::start();
    dev.wait_for_response("before-reload");

    dev.overwrite_script("after-reload");
    dev.wait_for_response("after-reload");

    dev.stop();
    let events = dev.events();
    let codes = events
        .iter()
        .map(|event| event["eventCode"].as_u64().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(codes, [1, 2, 3, 4, 5, 8]);
    assert!(events.iter().all(|event| event["schemaVersion"] == 1));
    assert!(events.iter().all(|event| event["surfaceCode"] == 1));
    assert!(events.windows(2).all(|pair| {
        pair[0]["sequence"].as_u64().unwrap() < pair[1]["sequence"].as_u64().unwrap()
    }));
}

fn http_get(port: u16) -> std::io::Result<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.write_all(b"GET /version HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}

fn php_source(version: &str) -> String {
    format!(
        r#"<?php

use Pam\Http\Request;
use Pam\Http\Response;
use Pam\Http\Server;

Server::create(static fn (Request $request, Response $response) =>
    $request->path === '/version'
        ? $response->json(['version' => '{version}'])
        : $response->json(['error' => 'Not Found'], 404)
)->listen((int) getenv('PAM_TEST_PORT'));
"#
    )
}
