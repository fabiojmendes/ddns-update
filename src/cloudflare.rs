use std::net::IpAddr;

use serde::Deserialize;
use serde_json::{Value, json};

use reqwest::{Client, Url};

#[derive(Debug, Deserialize)]
struct CFReponse {
    result: Vec<CFHost>,
}

#[derive(Debug, Deserialize)]
struct CFHost {
    id: String,
}

pub struct CloudflareClient {
    cf_token: String,
    base_url: Url,
    http_client: Client,
}

impl CloudflareClient {
    pub fn new(cf_token: String, zone_id: String) -> anyhow::Result<Self> {
        let base_url = Url::parse(&format!(
            "https://api.cloudflare.com/client/v4/zones/{}/dns_records/",
            zone_id
        ))?;
        let http_client = Client::new();
        Ok(CloudflareClient {
            cf_token,
            base_url,
            http_client,
        })
    }
    pub async fn update(&self, ip: &IpAddr, fqdn: &str) -> anyhow::Result<Value> {
        let resp = self
            .http_client
            .get(self.base_url.clone())
            .bearer_auth(&self.cf_token)
            .query(&[("name", fqdn), ("type", "AAAA")])
            .send()
            .await?;
        let cf_resp = resp.json::<CFReponse>().await?;
        let payload = json!({
            "type": "AAAA",
            "name": fqdn,
            "content": ip,
            "proxied": false,
        });
        let resp = match cf_resp.result.first() {
            Some(cf_host) => {
                log::info!("Update existing record with id: {}", cf_host.id);
                self.http_client
                    .put(self.base_url.join(&cf_host.id)?)
                    .bearer_auth(&self.cf_token)
                    .json(&payload)
                    .send()
                    .await?
                    .error_for_status()?
            }
            None => {
                log::info!("Create new dns record");
                self.http_client
                    .post(self.base_url.clone())
                    .bearer_auth(&self.cf_token)
                    .json(&payload)
                    .send()
                    .await?
                    .error_for_status()?
            }
        };
        Ok(resp.json::<Value>().await?)
    }
}
