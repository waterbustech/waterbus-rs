use aws_config::meta::region::RegionProviderChain;
use aws_credential_types::Credentials;
use aws_sdk_s3::{Client, config::Region};
use std::env;

pub fn get_storage_object_client() -> Client {
    dotenvy::dotenv().ok();
    let access_key_id = env::var("STORAGE_ACCESS_KEY_ID").expect("STORAGE_ACCESS_KEY_ID not set");
    let secret_access_key =
        env::var("STORAGE_SECRET_ACCESS_KEY").expect("STORAGE_SECRET_ACCESS_KEY not set");
    let region = env::var("STORAGE_REGION").ok();
    let endpoint_url = env::var("STORAGE_ENDPOINT_URL").ok();
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
    
    let rt = tokio::runtime::Handle::current();
    let shared_config = rt.block_on(async {
        aws_config::from_env()
            .region(region_provider)
            .endpoint_url(endpoint_url.unwrap_or_default())
            .credentials_provider(credentials)
            .load()
            .await
    });
    
    Client::new(&shared_config)
}