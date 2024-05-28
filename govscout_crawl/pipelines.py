from base64 import b64encode
import boto3
from botocore.exceptions import ClientError
from hashlib import md5, sha256
from itemadapter import ItemAdapter
from .items import WebsOpportunityItem, WebsVendorItem, WebsDocumentItem, webs_vendor_sha256
from mypy_boto3_dynamodb.client import DynamoDBClient
from mypy_boto3_s3.client import S3Client
from struct import pack
import re


class WebsAWSPipeline:
    def __init__(self, ddb: DynamoDBClient, s3: S3Client, table_prefix: str, s3_bucket: str, s3_prefix: str):
        self.ddb = ddb
        self.s3 = s3
        self.table_prefix = table_prefix
        self.s3_bucket = s3_bucket
        self.s3_prefix = s3_prefix
    
    @classmethod
    def from_crawler(cls, crawler):
        region = crawler.settings.get("AWS_REGION")
        access_key = crawler.settings.get("AWS_ACCESS_KEY_ID")
        secret_key = crawler.settings.get("AWS_SECRET_ACCESS_KEY")
        session_token = crawler.settings.get("AWS_SESSION_TOKEN")
        table_prefix = crawler.settings.get("DYNAMODB_TABLE_PREFIX", "")
        s3_url = crawler.settings.get("WEBS_DOCUMENTS_URL")

        if s3_url is None:
            raise ValueError("WEBS_DOCUMENTS_URL is required")

        m = re.match(r"s3://([^/]+)(?:/(.+))?", s3_url)
        if m is None:
            raise ValueError(f"Invalid WEBS_DOCUMENTS_URL: {s3_url}")

        s3_bucket = m.group(1)
        s3_prefix = m.group(2)
        if s3_prefix is None:
            s3_prefix = ""
        elif not s3_prefix.endswith("/"):
            s3_prefix += "/"

        kwargs = {}
        if region:
            kwargs["region_name"] = region
        if access_key:
            kwargs["aws_access_key_id"] = access_key
        if secret_key:
            kwargs["aws_secret_access_key"] = secret_key
        if session_token:
            kwargs["aws_session_token"] = session_token

        session = boto3.session.Session(**kwargs)
        dynamodb = session.client("dynamodb")
        s3 = session.client("s3")

        return cls(dynamodb, s3, table_prefix, s3_bucket, s3_prefix)

    def process_item(self, item, spider):
        item = ItemAdapter(item)
        type = item.get("type")
        if type:
            handler = self.handlers.get(type)
            if handler:
                handler(self, item, spider)
            else:
                spider.logger.warning(f"No handler for item type {type}")
        else:
            spider.logger.warning(f"No type field in item")

        return item

    def process_opportunity(self, item: WebsOpportunityItem, spider):
        opportunities_table_name = f"{self.table_prefix}WebsOpportunities"
        ddb_item = {"SystemId": {"S": item["system_id"]}}

        if item["customer_ref_num"]:
            ddb_item["CustomerRefNum"] = {"S": item["customer_ref_num"]}
        if item["org_name"]:
            ddb_item["OrgName"] = {"S": item["org_name"]}
        if item["title"]:
            ddb_item["Title"] = {"S": item["title"]}
        if item["description"]:
            ddb_item["Description"] = {"S": item["description"]}
        if item["date_posted"]:
            ddb_item["DatePosted"] = {"S": item["date_posted"]}
        if item["date_closed"]:
            ddb_item["DateClosed"] = {"S": item["date_closed"]}
        if item["estimated_value"]:
            ddb_item["EstimatedValue"] = {"S": item["estimated_value"]}
        if item["contact_name"]:
            ddb_item["ContactName"] = {"S": item["contact_name"]}
        if item["contact_phone"]:
            ddb_item["ContactPhone"] = {"S": item["contact_phone"]}
        if item["contact_email"]:
            ddb_item["ContactEmail"] = {"S": item["contact_email"]}
        if item["commodity_codes"]:
            ddb_item["CommodityCodes"] = {"SS": item["commodity_codes"]}
        if item["counties"]:
            ddb_item["Counties"] = {"SS": item["counties"]}

        try:
            self.ddb.put_item(TableName=opportunities_table_name, Item=ddb_item)
        except Exception as e:
            spider.logger.error(
                f"Failed to write opportunity to DynamoDB table {opportunities_table_name}: {e}"
            )
            raise

    def process_vendor(self, item: WebsVendorItem, spider):
        opportunities_table_name = f"{self.table_prefix}WebsOpportunities"
        vendors_table_name = f"{self.table_prefix}WebsVendors"
        sha256_hash = webs_vendor_sha256(item)

        vendor_item = {"VendorId": {"S": sha256_hash}}
        if item["company_name"]:
            vendor_item["CompanyName"] = {"S": item["company_name"]}
        if item["email"]:
            vendor_item["Email"] = {"S": item["email"]}
        if item["phone"]:
            vendor_item["Phone"] = {"S": item["phone"]}
        if item["status"]:
            vendor_item["Status"] = {"SS": item["status"]}

        try:
            self.ddb.put_item(TableName=vendors_table_name, Item=vendor_item)
        except Exception as e:
            spider.logger.error(
                f"Failed to write vendor to DynamoDB table {vendors_table_name}: {e}"
            )
            raise

        key = {"SystemId": {"S": item["system_id"]}}
        update_expression = "ADD Vendors :vendor_id"
        expression_attribute_values = {":vendor_id": {"SS": [sha256_hash]}}

        try:
            self.ddb.update_item(
                TableName=opportunities_table_name,
                Key=key,
                UpdateExpression=update_expression,
                ExpressionAttributeValues=expression_attribute_values,
            )
        except Exception as e:
            spider.logger.error(
                f"Failed to update opportunity in DynamoDB table {opportunities_table_name}: {e}"
            )
            raise

    def process_document(self, item: WebsDocumentItem, spider):
        body = item["contents"]
        hasher = sha256(body)
        md5_b64 = b64encode(md5(body).digest()).decode("utf-8")
        sha256_hex = hasher.hexdigest()
        sha256_b64 = b64encode(hasher.digest()).decode("utf-8")
        content_length = len(body)

        s3_key = f"{self.s3_prefix}{sha256_hex}"
        opportunities_table_name = f"{self.table_prefix}WebsOpportunities"
        documents_table_name = f"{self.table_prefix}Documents"

        try:
            self.s3.head_object(Bucket=self.s3_bucket, Key=s3_key, RequestPayer="requester")
        except ClientError:
            try:
                self.s3.put_object(
                    Bucket=self.s3_bucket,
                    Key=s3_key,
                    Body=body,
                    ChecksumSHA256=sha256_b64,
                    ContentLength=content_length,
                    ContentMD5=md5_b64,
                    ContentType="application/octet-stream", # Always use binary; we don't checksum on this here.
                    RequestPayer="requester",
                )
            except Exception as e:
                spider.logger.error(f"Failed to write document {item.filename} to s3://{self.s3_bucket}/{s3_key}: {e}")
                raise

        # Extend the hash with the filename and content-type
        original_url = item["original_url"]
        original_url_utf8 = original_url.encode("utf-8")
        filename = item["filename"]
        filename_utf8 = filename.encode("utf-8")
        content_type = item["content_type"]
        content_type_utf8 = content_type.encode("utf-8")

        hasher.update(pack("<Q", content_length))
        hasher.update(pack("<Q", len(original_url_utf8)))
        hasher.update(original_url_utf8)
        hasher.update(pack("<Q", len(filename_utf8)))
        hasher.update(filename_utf8)
        hasher.update(pack("<Q", len(content_type_utf8)))
        hasher.update(content_type_utf8)
        document_id = hasher.hexdigest()

        document_item = {
            "DocumentId": {"S": document_id},
            "Filename": {"S": item["filename"]},
            "ContentLength": {"N": str(len(item["contents"]))},
            "S3Bucket": {"S": self.s3_bucket},
            "S3Key": {"S": s3_key},
            "S3Url": {"S": f"s3://{self.s3_bucket}/{s3_key}"},
        }

        if content_type:
            document_item["ContentType"] = {"S": content_type}
        
        try:
            self.ddb.put_item(
                TableName=documents_table_name,
                Item=document_item
            )
        except Exception as e:
            spider.logger.error(f"Failed to write document to DynamoDB table {documents_table_name}: {e}")
            raise

        opportunity_key = {"SystemId": {"S": item["system_id"]}}
        update_expression = "ADD Documents :doc_id"
        expression_attribute_values = {":doc_id": {"SS": [document_id]}}
        try:
            self.ddb.update_item(
                TableName=opportunities_table_name,
                Key=opportunity_key,
                UpdateExpression=update_expression,
                ExpressionAttributeValues=expression_attribute_values,
            )
        except Exception as e:
            spider.logger.error(
                f"Failed to update opportunity in DynamoDB table {opportunities_table_name}: {e}"
            )
            raise

WebsAWSPipeline.handlers = {
    "WebsOpportunity": WebsAWSPipeline.process_opportunity,
    "WebsVendor": WebsAWSPipeline.process_vendor,
    "WebsDocument": WebsAWSPipeline.process_document,
}