pub type CommandResult<T> = Result<T, String>;

pub fn to_command_error(error: anyhow::Error) -> String {
    error.to_string()
}
