use {aws_smithy_runtime_api::client::result::SdkError, log::error, std::fmt::Debug};

/// If the result of an AWS API call is an error, log the result.
///
/// This performs additional information beyond the Smithy SdkError's Display implementation.
pub fn log_aws_err<O, E, R>(result: Result<O, SdkError<E, R>>, reason: &str) -> Result<O, SdkError<E, R>>
where
    E: Debug,
    R: Debug,
{
    if let Err(e) = &result {
        error!("{reason}: {}", aws_err_str(e));
    }

    result
}

/// Expand an AWS error into more detail.
pub fn aws_err_str<E, R>(e: &SdkError<E, R>) -> String
where
    E: Debug,
    R: Debug,
{
    match &e {
        SdkError::ConstructionFailure(c) => format!("SdkError::ConstructionFailure({c:?})"),
        SdkError::TimeoutError(t) => format!("SdkError::TimeoutError({t:?})"),
        SdkError::DispatchFailure(d) => format!("SdkError::DispatchFailure({d:?})"),
        SdkError::ResponseError(r) => format!("SdkError::ResponseError({r:?})"),
        SdkError::ServiceError(s) => format!("SdkError::ServiceError({s:?})"),
        _ => format!("{e:?}"),
    }
}
