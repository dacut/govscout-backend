use {aws_smithy_runtime_api::client::result::SdkError, log::error, std::fmt::Debug};

/// If the result of an AWS API call is an error, log the result.
///
/// This performs additional information beyond the Smithy SdkError's Display implementation.
pub fn log_aws_err<O, E, R>(result: Result<O, SdkError<E, R>>) -> Result<O, SdkError<E, R>>
where
    E: Debug,
    R: Debug,
{
    if let Err(e) = &result {
        match &e {
            SdkError::ConstructionFailure(c) => error!("SdkError::ConstructionFailure({c:?})"),
            SdkError::TimeoutError(t) => error!("SdkError::TimeoutError({t:?})"),
            SdkError::DispatchFailure(d) => error!("SdkError::DispatchFailure({d:?})"),
            SdkError::ResponseError(r) => error!("SdkError::ResponseError({r:?})"),
            SdkError::ServiceError(s) => error!("SdkError::ServiceError({s:?})"),
            _ => error!("{e:?}"),
        }
    }

    result
}
