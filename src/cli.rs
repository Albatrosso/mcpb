use std::ffi::OsString;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenCommand {
    pub browser_query: String,
    pub port: u16,
}

pub fn parse_args<I, T>(args: I) -> Result<OpenCommand, String>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let values = args
        .into_iter()
        .map(Into::into)
        .map(|value| value.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    if values.len() < 2 {
        return Err(usage_error());
    }

    if values[1] != "open" {
        return Err(usage_error());
    }

    let mut browser_query = None;
    let mut port = 9222;
    let mut index = 2;

    while index < values.len() {
        let value = &values[index];

        if value == "--help" || value == "-h" {
            return Err(usage_error());
        }

        if value == "--port" {
            let port_value = values
                .get(index + 1)
                .ok_or_else(|| String::from("Expected a port value after --port"))?;
            port = port_value
                .parse::<u16>()
                .map_err(|_| format!("Invalid port: {port_value}"))?;
            index += 2;
            continue;
        }

        if let Some(port_value) = value.strip_prefix("--port=") {
            port = port_value
                .parse::<u16>()
                .map_err(|_| format!("Invalid port: {port_value}"))?;
            index += 1;
            continue;
        }

        if let Some(flag_name) = value.strip_prefix("--") {
            if flag_name.is_empty() {
                return Err(usage_error());
            }

            if browser_query.replace(flag_name.to_string()).is_some() {
                return Err(String::from(
                    "Provide exactly one browser flag like --brave",
                ));
            }

            index += 1;
            continue;
        }

        return Err(usage_error());
    }

    let browser_query =
        browser_query.ok_or_else(|| String::from("Provide a browser flag like --brave"))?;

    Ok(OpenCommand {
        browser_query,
        port,
    })
}

fn usage_error() -> String {
    String::from("Usage: mcpb open --<browser> [--port <n>]")
}

#[cfg(test)]
mod tests {
    use super::parse_args;

    #[test]
    fn parses_open_command_with_browser_flag_and_port() {
        let command = parse_args(["mcpb", "open", "--BrAve", "--port", "9333"]).unwrap();

        assert_eq!(command.browser_query, "BrAve");
        assert_eq!(command.port, 9333);
    }

    #[test]
    fn rejects_missing_browser_flag() {
        let error = parse_args(["mcpb", "open"]).unwrap_err();

        assert!(error.contains("Provide a browser flag"));
    }

    #[test]
    fn rejects_multiple_browser_flags() {
        let error = parse_args(["mcpb", "open", "--brave", "--dia"]).unwrap_err();

        assert!(error.contains("exactly one browser flag"));
    }
}
