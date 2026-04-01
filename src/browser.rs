use crate::discovery::BrowserApp;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub fn is_running(executable_path: &Path) -> Result<bool, String> {
    let output = Command::new("ps")
        .arg("axww")
        .arg("-o")
        .arg("command=")
        .output()
        .map_err(|error| format!("Could not inspect browser processes: {error}"))?;

    if !output.status.success() {
        return Err(format!(
            "Could not inspect browser processes: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let executable_path = executable_path.to_string_lossy();
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| line.contains(executable_path.as_ref()) && !line.contains("--headless")))
}

pub fn read_confirmation() -> Result<bool, String> {
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| format!("Could not read confirmation: {error}"))?;

    let normalized = input.trim().to_ascii_lowercase();
    Ok(!matches!(normalized.as_str(), "n" | "no"))
}

pub fn quit_and_wait(bundle_id: &str, executable_path: &Path) -> Result<(), String> {
    let output = Command::new(osascript_binary())
        .arg("-e")
        .arg(format!(r#"tell application id "{}" to quit"#, bundle_id))
        .output()
        .map_err(|error| format!("Could not ask browser to quit: {error}"))?;

    if !output.status.success() {
        return Err(format!(
            "Could not ask browser to quit: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let timeout = Duration::from_millis(timeout_ms("MCPB_QUIT_TIMEOUT_MS", 10_000));
    let start = Instant::now();

    while start.elapsed() < timeout {
        if !is_running(executable_path)? {
            return Ok(());
        }

        thread::sleep(Duration::from_millis(100));
    }

    Err(String::from(
        "Browser could not be closed automatically. Please close it manually and try again.",
    ))
}

pub fn launch(app: &BrowserApp, port: u16) -> Result<(), String> {
    let profile_dir = profile_dir(&app.slug)?;
    fs::create_dir_all(&profile_dir)
        .map_err(|error| format!("Could not create {}: {error}", profile_dir.display()))?;

    Command::new(&app.executable_path)
        .arg(format!("--remote-debugging-port={port}"))
        .arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("about:blank")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not launch {}: {error}", app.display_name))?;

    Ok(())
}

pub fn wait_until_ready(port: u16) -> Result<(), String> {
    let timeout = Duration::from_millis(timeout_ms("MCPB_READY_TIMEOUT_MS", 15_000));
    let start = Instant::now();
    let ready_url = format!("http://127.0.0.1:{port}/json/version");

    while start.elapsed() < timeout {
        let result = ureq::get(&ready_url).call();
        if matches!(result, Ok(response) if response.status() == 200) {
            return Ok(());
        }

        thread::sleep(Duration::from_millis(100));
    }

    Err(format!(
        "Browser did not expose a ready MCP endpoint at {ready_url}"
    ))
}

pub fn print_success(display_name: &str, port: u16) {
    let browser_url = format!("http://127.0.0.1:{port}");
    let claude_desktop_json = format!(
        "{{\n  \"mcpServers\": {{\n    \"mcpb-browser\": {{\n      \"type\": \"stdio\",\n      \"command\": \"npx\",\n      \"args\": [\"-y\", \"chrome-devtools-mcp@latest\", \"--browser-url={browser_url}\"],\n      \"env\": {{}}\n    }}\n  }}\n}}"
    );
    let claude_code_command = format!(
        "claude mcp add-json mcpb-browser '{{\"type\":\"stdio\",\"command\":\"npx\",\"args\":[\"-y\",\"chrome-devtools-mcp@latest\",\"--browser-url={browser_url}\"],\"env\":{{}}}}'"
    );
    let codex_toml = format!(
        "[mcp_servers.mcpb_browser]\ncommand = \"npx\"\nargs = [\"-y\", \"chrome-devtools-mcp@latest\", \"--browser-url={browser_url}\"]\nenabled = true"
    );

    println!("All set. {display_name} launched in MCP mode.");
    println!("Debug endpoint: {browser_url}");
    println!();
    println!("Claude Desktop");
    println!("Merge this into ~/Library/Application Support/Claude/claude_desktop_config.json:");
    println!("{claude_desktop_json}");
    println!();
    println!("Claude Code");
    println!("Run:");
    println!("{claude_code_command}");
    println!();
    println!("Codex");
    println!("Add this to ~/.codex/config.toml:");
    println!("{codex_toml}");
}

fn osascript_binary() -> String {
    env::var("MCPB_OSASCRIPT_BIN").unwrap_or_else(|_| String::from("osascript"))
}

fn timeout_ms(key: &str, fallback: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(fallback)
}

fn profile_dir(slug: &str) -> Result<PathBuf, String> {
    let home = env::var("HOME").map_err(|_| String::from("HOME is not set"))?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("mcpb")
        .join("profiles")
        .join(slug))
}
