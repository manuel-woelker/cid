#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessOutputStream {
    Stdout,
    Stderr,
}
