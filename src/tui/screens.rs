#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    List,
    Detail,
    Doctor,
    History,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    None,
    NewProfile,
    ImportDefault,
    ImportSub2,
    DeleteConfirm,
    ContinueProfile,
    ExecPrompt,
    ShareConfirm,
    UnshareConfirm,
    UpdatePrompt,
    Message,
}
