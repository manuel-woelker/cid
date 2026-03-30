use cid_base::cli::try_main;
use cid_base::file_path::FilePath;
use cid_base::logging::{error, info, init_logging};
use cid_base::result::CidResult;
use cid_daemon::CidConfig;
use cid_pal::pal_real::PalReal;
use std::process::ExitCode;
use std::thread;

fn main() -> ExitCode {
    init_logging();
    try_main(run)
}

fn run() -> CidResult<()> {
    let pal = PalReal::new_handle();
    let config = CidConfig::load_from_path(&FilePath::new("cid-config.yaml"), &*pal)?;
    let mut daemon = cid_daemon::CidDaemon::from_config(&config, pal.clone())?;

    if config.web().enabled() {
        let address = config.web().address().to_string();
        let daemon_handle = daemon.handle();
        let web_pal = pal.clone();
        thread::spawn(move || {
            if let Err(error) = cid_web::serve(&address, daemon_handle, web_pal) {
                error!(error = %error.to_test_string(), "web server exited with an error");
            }
        });
        info!(
            address = %config.web().address(),
            "web server listening"
        );
    }

    daemon.run_forever(config.poll_interval())
}
