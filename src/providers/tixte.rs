use std::collections::HashMap;
use std::env;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use redis::{AsyncCommands, Client as RedisClient};
use reqwest::header::{HeaderValue, AUTHORIZATION};
use reqwest::multipart::{Form, Part};
use reqwest::Client;
use serde_json::Value;

use super::Provider;

#[derive(Debug)]
pub struct TixteProvider(Arc<RedisClient>, Client);

#[async_trait]
impl Provider for TixteProvider {
    fn new() -> Self {
        let redis = Arc::new(
            RedisClient::open(env::var("REDIS_URL").expect("Failed to load redis url"))
                .expect("Failed to open redis client"),
        );

        let client = Client::new();

        Self(redis, client)
    }

    #[inline]
    fn prefix() -> String {
        "tixte".to_owned()
    }

    async fn get(&self, slug: String) -> Result<Vec<u8>> {
        let mut con = self.0.get_async_connection().await?;
        let url: String = con.get(format!("{}:{slug}", TixteProvider::prefix())).await?;
        let data = self.1.get(url).send().await?.bytes().await?;

        Ok(data.as_ref().to_vec())
    }

    async fn set(&self, slug: String, data: Vec<u8>) -> Result<()> {
        let mut con = self.0.get_async_connection().await?;
        let file = Part::bytes(data).mime_str("image/png")?.file_name(format!("{slug}.png"));
        let form = Form::new().part("file", file);
        let domain_conf = match &env::var("TIXTE_DOMAIN_CONFIG")
            .expect("Failed to load tixte domain configuration")[..]
        {
            "standard" => DomainConfig::Standard(
                env::var("TIXTE_CUSTOM_DOMAIN").expect("Failed to load tixte custom domain"),
            ),
            "random" => DomainConfig::Random,
            _ => panic!("Invalid domain configuration"),
        };

        let url = match domain_conf {
            DomainConfig::Standard(domain) => {
                let mut params: HashMap<&'static str, bool> = HashMap::new();

                params.insert("random_name", false);

                let res = self
                    .1
                    .post("https://api.tixte.com/v1/upload")
                    .multipart(form)
                    .query(&params)
                    .header("domain", domain)
                    .header(
                        AUTHORIZATION,
                        env::var("TIXTE_UPLOAD_KEY")
                            .expect("Failed to load tixte upload key")
                            .parse::<HeaderValue>()
                            .expect("Failed to parse tixte upload key"),
                    )
                    .send()
                    .await?
                    .json::<Value>()
                    .await?;

                res["data"]["direct_url"].as_str().expect("Failed parsing direct url").to_owned()
            },
            DomainConfig::Random => {
                let mut params: HashMap<&'static str, bool> = HashMap::new();

                params.insert("random_name", false);
                params.insert("random", true);

                let res = self
                    .1
                    .post("https://api.tixte.com/v1/upload")
                    .multipart(form)
                    .query(&params)
                    .header(
                        AUTHORIZATION,
                        env::var("TIXTE_UPLOAD_KEY")
                            .expect("Failed to load tixte upload key")
                            .parse::<HeaderValue>()
                            .expect("Failed to parse tixte upload key"),
                    )
                    .send()
                    .await?
                    .json::<Value>()
                    .await?;

                res["data"]["direct_url"].as_str().expect("Failed parsing direct url").to_owned()
            },
        };

        con.set(format!("{}:{slug}", TixteProvider::prefix()), url).await?;

        Ok(())
    }
}

enum DomainConfig {
    Standard(String),
    Random,
}
