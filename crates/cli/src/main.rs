use std::thread;

use cid_base::cli::try_main;
use cid_base::file_path::FilePath;
use cid_base::result::CidResult;
use cid_daemon::CidConfig;
use std::process::ExitCode;

fn main() -> ExitCode {
    try_main(run)
}

fn run() -> CidResult<()> {
    let config = CidConfig::load_from_path(&FilePath::new("cid-config.yaml"))?;
    let mut daemon = cid_daemon::CidDaemon::from_config(&config)?;

    if config.web().enabled() {
        let address = config.web().address().to_string();
        let state_dir = config.state_dir().clone();
        thread::spawn(move || {
            if let Err(error) = cid_web::serve(&address, state_dir) {
                eprintln!("{}", error.to_test_string());
            }
        });
        println!(
            "cid: web server listening on http://{}",
            config.web().address()
        );
    }

    loop {
        let report = daemon.run_cycle()?;
        println!(
            "cid: {} repositories, {} discovered commits, {} queued runs, {} executed runs",
            daemon.repositories().len(),
            report.discovered_commits,
            report.queued_runs,
            report.executed_runs
        );
        thread::sleep(config.poll_interval());
    }
}
