mod browser;
mod cli;
mod discovery;

use std::ffi::OsString;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run(std::env::args_os()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run<I, T>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let command = cli::parse_args(args)?;
    let app = discovery::find_browser(&command.browser_query)?;

    if browser::is_running(&app.executable_path)? {
        println!(
            "Browser \"{}\" must be restarted for MCP mode. Close it now? [Y/n]",
            app.display_name
        );

        let confirmed = browser::read_confirmation()?;
        if !confirmed {
            return Ok(());
        }

        browser::quit_and_wait(&app.bundle_id, &app.executable_path)?;
    }

    browser::launch(&app, command.port)?;
    browser::wait_until_ready(command.port)?;
    browser::print_success(&app.display_name, command.port);
    Ok(())
}
