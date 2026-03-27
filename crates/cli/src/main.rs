use cid_base::cli::try_main;
use cid_base::result::CidResult;
use std::process::ExitCode;

fn main() -> ExitCode {
    try_main(run)
}

fn run() -> CidResult<()> {
    println!("cid");
    Ok(())
}
