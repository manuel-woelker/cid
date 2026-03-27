use cid_base::cli::try_main;
use cid_base::file_path::FilePath;
use cid_base::result::CidResult;
use cid_daemon::{CidConfig, CidDaemon};
use std::process::ExitCode;

fn main() -> ExitCode {
    try_main(run)
}

fn run() -> CidResult<()> {
    let config = CidConfig::load_from_path(&FilePath::new("cid-config.yaml"))?;
    let mut daemon = CidDaemon::new();

    for repository in config.repositories()? {
        daemon.add_repository(repository);
    }

    println!(
        "cid: loaded {} repositories from cid-config.yaml",
        daemon.repositories().len()
    );

    Ok(())
}
