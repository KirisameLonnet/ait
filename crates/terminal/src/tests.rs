use super::Error;
use model::ErrorCode;

pub(crate) mod support;

#[test]
fn terminal_failures_keep_their_public_error_categories() {
    for (error, expected) in [
        (Error::Invalid, ErrorCode::InvalidMessage),
        (Error::NotFound, ErrorCode::TerminalNotFound),
        (Error::WorkspaceNotFound, ErrorCode::WorkspaceNotFound),
        (Error::Registry, ErrorCode::RegistryIo),
        (Error::Exhausted, ErrorCode::ResourceExhausted),
        (Error::Io, ErrorCode::TerminalIo),
        (Error::MethodNotFound, ErrorCode::MethodNotFound),
    ] {
        assert_eq!(ErrorCode::from(error), expected);
    }
}
