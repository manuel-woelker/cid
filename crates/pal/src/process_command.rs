use cid_base::file_path::FilePath;
use cid_base::shared_string::SharedString;

use crate::process_environment_variable::ProcessEnvironmentVariable;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProcessCommand {
    pub executable: SharedString,
    pub arguments: Vec<SharedString>,
    pub working_directory: Option<FilePath>,
    pub environment: Vec<ProcessEnvironmentVariable>,
}
