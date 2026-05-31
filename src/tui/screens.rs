#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    List,
    Doctor,
    History,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    None,
    AddAccountMethod,
    LoginMethodForNewAccount,
    LoginMethodForSelected,
    NewLoginProfileName,
    ImportDefault,
    ImportSub2,
    DeleteConfirm,
    ContinueProfile,
    UpdatePrompt,
    Message,
}
