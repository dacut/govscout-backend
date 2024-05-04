//! AWS Lambda backend for the GovScout crawler.
//!
#![warn(clippy::all)]
#![deny(rustdoc::missing_crate_level_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(missing_docs)]

/// HTTP extension utilities.
pub mod httpext;

/// Shapes used in the request.
pub mod shapes;

/// HTML parsing library.
pub mod soup;

/// Washington State Electronic Business Solution (WEBS) service functionality.
pub mod webs;

use {
    crate::{
        httpext::{log_aws_err, LogConfig},
        shapes::{Operation, Request, Response},
    },
    aws_lambda_events::sqs::SqsEventObj,
    aws_sdk_sqs::types::{
        MessageAttributeValue, MessageSystemAttributeNameForSends, MessageSystemAttributeValue,
        SendMessageBatchRequestEntry,
    },
    futures::stream::FuturesUnordered,
    lambda_runtime::{run, service_fn, Context, Error as LambdaError, LambdaEvent},
    log::*,
    std::{error::Error, str::FromStr},
    uuid::{NoContext, Timestamp, Uuid},
};

const MSG_ATTR_SUBSYSTEM: &str = "Subsystem";
const MSG_ATTR_OPERATION: &str = "Operation";
const MSG_DATA_TYPE_STRING: &str = "String";
const MAX_SQS_BATCH_SIZE: usize = 10;

/// Dynamic error type that is safe to send across threads.
pub type BoxError = Box<dyn Error + Send + Sync>;

#[tokio::main]
async fn main() -> Result<(), LambdaError> {
    env_logger::init();
    let func = service_fn(handler);
    run(func).await?;
    Ok(())
}

async fn handler(event: LambdaEvent<SqsEventObj<Request>>) -> Result<(), LambdaError> {
    let (request, context) = event.into_parts();
    let futures = FuturesUnordered::new();
    let log_config = LogConfig::new().await;

    for record in request.records.into_iter() {
        info!("Received record {record:?}");
        let request = record.body;
        futures.push(Box::pin(dispatch(log_config.clone(), request, context.clone())));
    }

    let mut next_requests = Vec::with_capacity(futures.len() * 5);
    let mut errors = vec![];

    for future in futures.into_iter() {
        match future.await {
            Ok(response) => {
                for next_request in response.next_requests {
                    next_requests.push(next_request);
                }
            }
            Err(error) => errors.push(error),
        }
    }

    match errors.len() {
        0 => {
            info!("All futures completed successfully");

            let timestamp = Timestamp::now(NoContext);

            let mut batch_size = 0;
            let send_message_batch_base =
                log_config.sqs_client.send_message_batch().queue_url(&log_config.sqs_queue_url);
            let mut send_message_batch = send_message_batch_base.clone();

            for next_request in next_requests {
                let id = Uuid::new_v7(timestamp);
                let message_body = serde_json::to_string(&next_request).unwrap();
                let subsystem = MessageAttributeValue::builder()
                    .string_value(next_request.operation.subsystem())
                    .data_type(MSG_DATA_TYPE_STRING)
                    .build()
                    .unwrap();
                let operation = MessageAttributeValue::builder()
                    .string_value(next_request.operation.operation())
                    .data_type(MSG_DATA_TYPE_STRING)
                    .build()
                    .unwrap();

                let mut message = SendMessageBatchRequestEntry::builder()
                    .id(id)
                    .message_body(message_body)
                    .delay_seconds(0)
                    .message_attributes(MSG_ATTR_SUBSYSTEM, subsystem)
                    .message_attributes(MSG_ATTR_OPERATION, operation);
                if let Some(xray_trace_id) = context.xray_trace_id.as_ref() {
                    let xray_trace_id = MessageSystemAttributeValue::builder()
                        .string_value(xray_trace_id)
                        .data_type(MSG_DATA_TYPE_STRING)
                        .build()
                        .unwrap();
                    message = message
                        .message_system_attributes(MessageSystemAttributeNameForSends::AwsTraceHeader, xray_trace_id);
                }

                let message = message.build().unwrap();
                send_message_batch = send_message_batch.entries(message);
                batch_size += 1;

                if batch_size == MAX_SQS_BATCH_SIZE {
                    log_aws_err(send_message_batch.send().await, "SendMessageBatch")?;
                    send_message_batch = send_message_batch_base.clone();
                    batch_size = 0;
                }
            }

            if batch_size > 0 {
                log_aws_err(send_message_batch.send().await, "SendMessageBatch")?;
            }

            Ok(())
        }
        1 => {
            let e = errors.pop().unwrap();
            error!("Error: {e}");
            Err(e)
        }
        _ => {
            for e in errors.iter() {
                error!("Error: {e}");
            }
            Err("Multiple errors".into())
        }
    }
}

async fn dispatch(log_config: LogConfig, request: Request, context: Context) -> Result<Response, LambdaError> {
    let Ok(operation) = Operation::from_str(&request.operation) else {
        return Err(format!("Invalid operation: {}", request.operation).into());
    };
    operation.handle(log_config, request, context).await
}
