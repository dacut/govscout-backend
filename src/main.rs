#![warn(clippy::all)]

pub mod httpext;
pub mod shapes;
pub mod soup;
pub mod webs;

use {
    crate::{
        httpext::LogConfig,
        shapes::{Request, Response},
    },
    lambda_runtime::{run, service_fn, Error as LambdaError, LambdaEvent},
    log::*,
    std::error::Error,
};

pub type BoxError = Box<dyn Error + Send + Sync>;

#[tokio::main]
async fn main() -> Result<(), LambdaError> {
    env_logger::init();
    let func = service_fn(handler);
    run(func).await?;
    Ok(())
}

async fn handler(event: LambdaEvent<Request>) -> Result<Response, LambdaError> {
    let (request, context) = event.into_parts();

    let log_config = LogConfig::new().await;

    let result = match request.operation.as_str() {
        "StartWebsCrawl" => webs::start_crawl(log_config, request, context).await,
        _ => Err(format!("Unknown operation {}", request.operation).into()),
    };

    match result {
        Ok(ref response) => debug!("Returning response: {response:?}"),
        Err(ref error) => error!("Returning error: {error}"),
    }

    result
}
