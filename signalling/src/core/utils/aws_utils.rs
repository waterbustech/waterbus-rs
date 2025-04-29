use aws_config::meta::region::RegionProviderChain;
use aws_sdk_s3::{Client, config::Region};

pub async fn get_s3_client(region: Option<String>) -> Client {
    println!(
        "AWS_ACCESS_KEY_ID: {}",
        std::env::var("AWS_ACCESS_KEY_ID").unwrap()
    );
    println!(
        "AWS_SECRET_ACCESS_KEY: {}",
        std::env::var("AWS_SECRET_ACCESS_KEY").unwrap()
    );

    let region_provider = RegionProviderChain::first_try(region.map(Region::new))
        .or_default_provider()
        .or_else(Region::new("us-west-2"));

    let shared_config = aws_config::from_env().region(region_provider).load().await;
    let client = Client::new(&shared_config);

    client
}
