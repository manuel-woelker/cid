use std::thread;

use cid_base::cli::try_main;
use cid_base::file_path::FilePath;
use cid_base::result::CidResult;
use cid_daemon::CidConfig;
use cid_pal::pal_real::PalReal;
use std::process::ExitCode;

fn main() -> ExitCode {
    try_main(run)
}

fn run() -> CidResult<()> {
    let pal = PalReal::new_handle();
    let config = CidConfig::load_from_path(&FilePath::new("cid-config.yaml"), &*pal)?;
    let mut daemon = cid_daemon::CidDaemon::from_config(&config, pal.clone())?;

    if config.web().enabled() {
        let address = config.web().address().to_string();
        let state_dir = config.state_dir().clone();
        let web_pal = pal.clone();
        thread::spawn(move || {
            if let Err(error) = cid_web::serve(&address, state_dir, web_pal) {
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
        daemon.sleep(config.poll_interval());
    }
}
