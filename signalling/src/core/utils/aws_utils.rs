use aws_config::meta::region::RegionProviderChain;
use aws_credential_types::Credentials;
use aws_sdk_s3::{Client, config::Region};
use std::env;

pub async fn get_storage_object_client() -> Client {
    dotenvy::dotenv().ok();

    let access_key_id = env::var("AWS_ACCESS_KEY_ID").expect("AWS_ACCESS_KEY_ID not set");
    let secret_access_key =
        env::var("AWS_SECRET_ACCESS_KEY").expect("AWS_SECRET_ACCESS_KEY not set");
    let region = env::var("AWS_REGION").ok();
    let endpoint_url = env::var("AWS_ENDPOINT_URL").ok();

    let credentials = Credentials::new(
        access_key_id,
        secret_access_key,
        None,
        None,
        "waterbus_provider",
    );

    let region_provider = RegionProviderChain::first_try(region.map(Region::new))
        .or_default_provider()
        .or_else(Region::new("us-west-2"));

    let shared_config = aws_config::from_env()
        .region(region_provider)
        .endpoint_url(endpoint_url.unwrap_or_default())
        .credentials_provider(credentials)
        .load()
        .await;

    let client = Client::new(&shared_config);

    client
}
