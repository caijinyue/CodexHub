#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    List,
    Detail,
    Doctor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    None,
    NewProfile,
    ImportDefault,
    DeleteConfirm,
    ExecPrompt,
    ShareConfirm,
    UnshareConfirm,
    Message,
}
