#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    List,
    Doctor,
    History,
    Remote,
    SharedAccounts,
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
    RemoveSharedAccountConfirm,
    ImportSharedAccount,
    DeleteConfirm,
    DeleteSessionConfirm,
    ContinueProfile,
    CopySessionProfile,
    ProxyHttp,
    ProxyHttps,
    ProxyAll,
    ProxyNoProxy,
    UpdatePrompt,
    Message,
}
