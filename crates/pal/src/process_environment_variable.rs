use cid_base::shared_string::SharedString;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProcessEnvironmentVariable {
    pub name: SharedString,
    pub value: SharedString,
}
