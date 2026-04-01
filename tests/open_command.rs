use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command as ProcessCommand, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

#[test]
fn rejects_missing_browser_flag() {
    let mut command = binary();

    command.arg("open");

    command
        .assert()
        .failure()
        .stderr(predicate::str::contains("Provide a browser flag"));
}

#[test]
fn exits_cleanly_when_browser_restart_is_declined() {
    let fixture = Fixture::new();
    let port = free_port();
    let executable_path = fixture.write_fake_app(
        "Dia.app",
        "Dia",
        "Dia",
        "company.thebrowser.dia",
        FakeBrowserMode::Ready,
    );
    let mut running_browser = start_fake_browser(&fixture, &executable_path, port);

    let mut command = fixture.command();
    command
        .arg("open")
        .arg("--dia")
        .arg("--port")
        .arg(port.to_string())
        .write_stdin("n\n");

    command
        .assert()
        .success()
        .stdout(predicate::str::contains("must be restarted"))
        .stdout(predicate::str::contains("[Y/n]"))
        .stdout(predicate::str::contains("All set.").not());

    terminate(&mut running_browser);
}

#[test]
fn restarts_running_browser_and_prints_ready_snippets() {
    let fixture = Fixture::new();
    let port = free_port();
    let executable_path = fixture.write_fake_app(
        "Dia.app",
        "Dia",
        "Dia",
        "company.thebrowser.dia",
        FakeBrowserMode::Ready,
    );
    let mut running_browser = start_fake_browser(&fixture, &executable_path, port);

    let mut command = fixture.command();
    command
        .arg("open")
        .arg("--dia")
        .arg("--port")
        .arg(port.to_string())
        .write_stdin("Y\n");

    command
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "All set. Dia launched in MCP mode.",
        ))
        .stdout(predicate::str::contains(format!("http://127.0.0.1:{port}")))
        .stdout(predicate::str::contains("claude mcp add-json"))
        .stdout(predicate::str::contains("[mcp_servers.mcpb_browser]"));

    terminate_pid_file_process(&fixture.pid_file);
    terminate(&mut running_browser);
}

#[test]
fn fails_when_debug_endpoint_does_not_come_up() {
    let fixture = Fixture::new();
    let port = free_port();
    fixture.write_fake_app(
        "Brave Browser.app",
        "Brave",
        "Brave Browser",
        "com.brave.Browser",
        FakeBrowserMode::NoReadyEndpoint,
    );

    let mut command = fixture.command();
    command
        .arg("open")
        .arg("--brave")
        .arg("--port")
        .arg(port.to_string())
        .env("MCPB_READY_TIMEOUT_MS", "300");

    command
        .assert()
        .failure()
        .stderr(predicate::str::contains(format!(
            "http://127.0.0.1:{port}/json/version",
        )));

    terminate_pid_file_process(&fixture.pid_file);
}

#[test]
fn prints_port_override_in_ready_output() {
    let fixture = Fixture::new();
    let port = free_port();
    fixture.write_fake_app(
        "Brave Browser.app",
        "Brave",
        "Brave Browser",
        "com.brave.Browser",
        FakeBrowserMode::Ready,
    );

    let mut command = fixture.command();
    command
        .arg("open")
        .arg("--brave")
        .arg("--port")
        .arg(port.to_string());

    command
        .assert()
        .success()
        .stdout(predicate::str::contains(format!("http://127.0.0.1:{port}")))
        .stdout(predicate::str::contains(format!(
            "--browser-url=http://127.0.0.1:{port}",
        )));

    terminate_pid_file_process(&fixture.pid_file);
}

#[test]
fn keeps_stderr_quiet_while_waiting_for_browser_ready_non_interactively() {
    let fixture = Fixture::new();
    let port = free_port();
    fixture.write_fake_app(
        "Brave Browser.app",
        "Brave",
        "Brave Browser",
        "com.brave.Browser",
        FakeBrowserMode::DelayedReady,
    );

    let mut command = fixture.command();
    command
        .arg("open")
        .arg("--brave")
        .arg("--port")
        .arg(port.to_string());

    command
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "All set. Brave launched in MCP mode.",
        ))
        .stderr(predicate::str::is_empty());

    terminate_pid_file_process(&fixture.pid_file);
}

#[test]
fn ignores_headless_browser_processes_when_deciding_to_restart() {
    let fixture = Fixture::new();
    let port = free_port();
    let executable_path = fixture.write_fake_app(
        "Brave Browser.app",
        "Brave",
        "Brave Browser",
        "com.brave.Browser",
        FakeBrowserMode::Ready,
    );
    let mut headless_browser = ProcessCommand::new(&executable_path)
        .arg("--headless=new")
        .env("MCPB_FAKE_PID_FILE", &fixture.pid_file)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    wait_for_pid_file(&fixture.pid_file);

    let mut command = fixture.command();
    command
        .arg("open")
        .arg("--brave")
        .arg("--port")
        .arg(port.to_string());

    command
        .assert()
        .success()
        .stdout(predicate::str::contains("must be restarted").not())
        .stdout(predicate::str::contains(
            "All set. Brave launched in MCP mode.",
        ));

    terminate_pid_file_process(&fixture.pid_file);
    terminate(&mut headless_browser);
}

fn binary() -> Command {
    Command::cargo_bin("mcpb").unwrap()
}

struct Fixture {
    temp_dir: TempDir,
    applications_dir: PathBuf,
    osascript_path: PathBuf,
    pid_file: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let home_dir = temp_dir.path().join("home");
        let applications_dir = temp_dir.path().join("Applications");
        fs::create_dir_all(&home_dir).unwrap();
        fs::create_dir_all(&applications_dir).unwrap();

        let bin_dir = temp_dir.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let pid_file = temp_dir.path().join("fake-browser.pid");
        let osascript_path = bin_dir.join("osascript");
        fs::write(
            &osascript_path,
            r#"#!/bin/sh
set -eu
all_args="$*"
if printf '%s' "$all_args" | grep -q "is running"; then
  if [ -f "$MCPB_FAKE_PID_FILE" ] && kill -0 "$(cat "$MCPB_FAKE_PID_FILE")" >/dev/null 2>&1; then
    printf 'true\n'
  else
    printf 'false\n'
  fi
  exit 0
fi
if printf '%s' "$all_args" | grep -q " to quit"; then
  if [ -f "$MCPB_FAKE_PID_FILE" ]; then
    kill "$(cat "$MCPB_FAKE_PID_FILE")" >/dev/null 2>&1 || true
  fi
  exit 0
fi
printf 'false\n'
"#,
        )
        .unwrap();
        let mut permissions = fs::metadata(&osascript_path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&osascript_path, permissions).unwrap();

        Self {
            temp_dir,
            applications_dir,
            osascript_path,
            pid_file,
        }
    }

    fn command(&self) -> Command {
        let mut command = binary();
        command
            .env("HOME", self.temp_dir.path().join("home"))
            .env("MCPB_APPLICATION_DIRS", &self.applications_dir)
            .env("MCPB_OSASCRIPT_BIN", &self.osascript_path)
            .env("MCPB_READY_TIMEOUT_MS", "5000")
            .env("MCPB_QUIT_TIMEOUT_MS", "800")
            .env("MCPB_FAKE_PID_FILE", &self.pid_file);
        command
    }

    fn write_fake_app(
        &self,
        app_dir_name: &str,
        display_name: &str,
        executable_name: &str,
        bundle_id: &str,
        mode: FakeBrowserMode,
    ) -> PathBuf {
        let contents_dir = self.applications_dir.join(app_dir_name).join("Contents");
        let macos_dir = contents_dir.join("MacOS");
        fs::create_dir_all(&macos_dir).unwrap();

        let executable_path = macos_dir.join(executable_name);
        let script = match mode {
            FakeBrowserMode::Ready => ready_script(),
            FakeBrowserMode::DelayedReady => delayed_ready_script(),
            FakeBrowserMode::NoReadyEndpoint => no_ready_script(),
        };
        fs::write(&executable_path, script).unwrap();
        let mut permissions = fs::metadata(&executable_path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable_path, permissions).unwrap();

        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>{display_name}</string>
  <key>CFBundleExecutable</key>
  <string>{executable_name}</string>
  <key>CFBundleIdentifier</key>
  <string>{bundle_id}</string>
</dict>
</plist>
"#
        );
        fs::write(contents_dir.join("Info.plist"), plist).unwrap();

        executable_path
    }
}

#[derive(Clone, Copy)]
enum FakeBrowserMode {
    Ready,
    DelayedReady,
    NoReadyEndpoint,
}

fn start_fake_browser(fixture: &Fixture, executable_path: &Path, port: u16) -> Child {
    let child = ProcessCommand::new(executable_path)
        .arg(format!("--remote-debugging-port={port}"))
        .env("MCPB_FAKE_PID_FILE", &fixture.pid_file)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    wait_for_pid_file(&fixture.pid_file);
    child
}

fn terminate(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn terminate_pid_file_process(pid_file: &Path) {
    if let Ok(pid) = fs::read_to_string(pid_file) {
        let _ = ProcessCommand::new("kill").arg(pid.trim()).status();
    }
}

fn ready_script() -> &'static str {
    r#"#!/bin/sh
set -eu
pid_file="${MCPB_FAKE_PID_FILE:-}"
if [ -n "$pid_file" ]; then
  printf '%s' "$$" > "$pid_file"
fi
cleanup() {
  if [ -n "${server_pid:-}" ]; then
    kill "$server_pid" >/dev/null 2>&1 || true
  fi
  if [ -n "$pid_file" ]; then
    rm -f "$pid_file"
  fi
  exit 0
}
trap cleanup EXIT TERM INT
port=""
for arg in "$@"; do
  case "$arg" in
    --remote-debugging-port=*)
      port="${arg#*=}"
      ;;
  esac
done
if [ -z "$port" ]; then
  while true; do
    sleep 1
  done
fi
/usr/bin/python3 - "$port" <<'PY' &
import http.server
import json
import socketserver
import sys

port = int(sys.argv[1])

class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/json/version":
            payload = json.dumps({"Browser": "Fake Browser", "webSocketDebuggerUrl": f"ws://127.0.0.1:{port}/devtools/browser/fake"}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
            return
        self.send_response(404)
        self.end_headers()

    def log_message(self, format, *args):
        return

with socketserver.TCPServer(("127.0.0.1", port), Handler) as httpd:
    httpd.serve_forever()
PY
server_pid=$!
while true; do
  sleep 1
done
"#
}

fn no_ready_script() -> &'static str {
    r#"#!/bin/sh
set -eu
pid_file="${MCPB_FAKE_PID_FILE:-}"
if [ -n "$pid_file" ]; then
  printf '%s' "$$" > "$pid_file"
fi
cleanup() {
  if [ -n "$pid_file" ]; then
    rm -f "$pid_file"
  fi
  exit 0
}
trap cleanup EXIT TERM INT
while true; do
  sleep 1
done
"#
}

fn delayed_ready_script() -> &'static str {
    r#"#!/bin/sh
set -eu
pid_file="${MCPB_FAKE_PID_FILE:-}"
if [ -n "$pid_file" ]; then
  printf '%s' "$$" > "$pid_file"
fi
cleanup() {
  if [ -n "${server_pid:-}" ]; then
    kill "$server_pid" >/dev/null 2>&1 || true
  fi
  if [ -n "$pid_file" ]; then
    rm -f "$pid_file"
  fi
  exit 0
}
trap cleanup EXIT TERM INT
port=""
for arg in "$@"; do
  case "$arg" in
    --remote-debugging-port=*)
      port="${arg#*=}"
      ;;
  esac
done
if [ -z "$port" ]; then
  while true; do
    sleep 1
  done
fi
sleep 0.4
/usr/bin/python3 - "$port" <<'PY' &
import http.server
import json
import socketserver
import sys

port = int(sys.argv[1])

class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/json/version":
            payload = json.dumps({"Browser": "Fake Browser", "webSocketDebuggerUrl": f"ws://127.0.0.1:{port}/devtools/browser/fake"}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
            return
        self.send_response(404)
        self.end_headers()

    def log_message(self, format, *args):
        return

with socketserver.TCPServer(("127.0.0.1", port), Handler) as httpd:
    httpd.serve_forever()
PY
server_pid=$!
while true; do
  sleep 1
done
"#
}

fn wait_for_pid_file(pid_file: &Path) {
    let deadline = Instant::now() + Duration::from_secs(2);

    while Instant::now() < deadline {
        if pid_file.exists() {
            return;
        }

        thread::sleep(Duration::from_millis(20));
    }

    panic!("Timed out waiting for fake browser pid file");
}

fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}
