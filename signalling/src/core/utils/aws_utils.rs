use aws_config::{BehaviorVersion, SdkConfig};
use aws_credential_types::Credentials;
use aws_sdk_s3::{
    Client,
    config::{Region, SharedCredentialsProvider},
};
use std::env;

pub fn get_storage_object_client() -> (Client, String, Option<String>) {
    dotenvy::dotenv().ok();

    let access_key_id = env::var("STORAGE_ACCESS_KEY").expect("STORAGE_ACCESS_KEY not set");
    let secret_access_key = env::var("STORAGE_SECRET_KEY").expect("STORAGE_SECRET_KEY not set");
    let region = env::var("STORAGE_REGION").unwrap_or_else(|_| "auto".to_string());
    let endpoint_url = env::var("STORAGE_ENDPOINT").ok();
    let custom_domain = env::var("STORAGE_CUSTOM_DOMAIN").ok();
    let bucket_name = env::var("STORAGE_BUCKET").expect("STORAGE_BUCKET must be set");

    let credentials = Credentials::new(
        access_key_id,
        secret_access_key,
        None,
        None,
        "waterbus_provider",
    );

    let config = SdkConfig::builder()
        .behavior_version(BehaviorVersion::latest())
        .endpoint_url(endpoint_url.unwrap_or_default())
        .region(Region::new(region))
        .credentials_provider(SharedCredentialsProvider::new(credentials))
        .build();

    let client = Client::new(&config);

    (client, bucket_name, custom_domain)
}
