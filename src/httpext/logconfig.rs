use {
    crate::{httpext::log_aws_err, BoxError},
    aws_sdk_dynamodb::Client as DynamoDbClient,
    aws_sdk_s3::Client as S3Client,
    aws_sdk_ssm::Client as SsmClient,
    log::*,
    std::env,
};

const ENV_LOG_S3_BUCKET: &str = "LOG_S3_BUCKET";
const ENV_LOG_S3_PREFIX: &str = "LOG_S3_PREFIX";
const ENV_LOG_DDB_TABLE: &str = "LOG_DYNAMODB_TABLE";
const ENV_LOG_DYNAMODB_TABLE: &str = "LOG_DYNAMODB_TABLE";
const ENV_SSM_PREFIX: &str = "SSM_PREFIX";
const DEFAULT_SSM_PREFIX: &str = "/GovScout/";

/// Configuration for logging requests and responses.
#[derive(Clone, Debug)]
pub struct LogConfig {
    /// The DynamoDB client to use.
    pub ddb_client: DynamoDbClient,

    /// The S3 client to use.
    pub s3_client: S3Client,

    /// The Systems Manager client to use.
    pub ssm_client: SsmClient,

    /// The S3 bucket to log to.
    pub s3_bucket: String,

    /// The S3 key prefix to use.
    pub s3_prefix: String,

    /// The SSM prefix to use.
    pub ssm_prefix: String,

    /// The DynamoDB table to use.
    pub ddb_table: String,
}

impl LogConfig {
    /// Create a new LogConfig from environment variables.
    pub async fn new() -> Self {
        let aws_config = aws_config::load_from_env().await;
        let ddb_client = DynamoDbClient::new(&aws_config);
        let s3_client = S3Client::new(&aws_config);
        let ssm_client = SsmClient::new(&aws_config);

        let s3_bucket = env::var(ENV_LOG_S3_BUCKET).expect("LOG_S3_BUCKET must be set");
        let s3_prefix = env::var(ENV_LOG_S3_PREFIX).unwrap_or_else(|_| "".to_string());
        let ssm_prefix = env::var(ENV_SSM_PREFIX).unwrap_or_else(|_| DEFAULT_SSM_PREFIX.to_string());
        let ddb_table = env::var(ENV_LOG_DYNAMODB_TABLE)
            .unwrap_or_else(|_| env::var(ENV_LOG_DDB_TABLE).expect("LOG_DYNAMODB_TABLE or LOG_DDB_TABLE must be set"));

        Self {
            ddb_client,
            s3_client,
            ssm_client,
            s3_bucket,
            s3_prefix,
            ssm_prefix,
            ddb_table,
        }
    }

    /// Get a parameter or return an error.
    pub async fn get_parameter(&self, name: &str) -> Result<String, BoxError> {
        let parameter_name = format!("{}{}", self.ssm_prefix, name);
        debug!("Retrieving SSM parameter {parameter_name}");
        let result =
            log_aws_err(self.ssm_client.get_parameter().name(&parameter_name).with_decryption(true).send().await)?;
        let Some(param) = result.parameter else {
            return Err(format!("Parameter {} not found", parameter_name).into());
        };

        let Some(value) = param.value else {
            return Err(format!("Parameter {} has no value", parameter_name).into());
        };

        Ok(value)
    }
}
