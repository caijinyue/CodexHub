#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    List,
    Doctor,
    History,
    Settings,
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
    ShareTargetUser,
    ShareNeedsSudo,
    ImportSharedAccount,
    DeleteConfirm,
    ContinueProfile,
    UpdatePrompt,
    Message,
}
