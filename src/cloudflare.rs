use std::net::Ipv6Addr;

use serde::Deserialize;
use serde_json::{Value, json};

use reqwest::{Url, blocking::Client};

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
    pub fn update(&self, ip: &Ipv6Addr, fqdn: &str) -> anyhow::Result<Value> {
        let resp = self
            .http_client
            .get(self.base_url.clone())
            .bearer_auth(&self.cf_token)
            .query(&[("name", fqdn), ("type", "AAAA")])
            .send()?;
        let cf_resp = resp.json::<CFReponse>()?;
        let payload = json!({
            "type": "AAAA",
            "name": fqdn,
            "content": ip,
            "proxied": false,
        });
        let resp = match cf_resp.result.first() {
            Some(cf_host) => {
                println!("Update existing record with id: {}", cf_host.id);
                self.http_client
                    .put(self.base_url.join(&cf_host.id)?)
                    .bearer_auth(&self.cf_token)
                    .json(&payload)
                    .send()?
                    .error_for_status()?
            }
            None => {
                println!("Create new dns record");
                self.http_client
                    .post(self.base_url.clone())
                    .bearer_auth(&self.cf_token)
                    .json(&payload)
                    .send()?
                    .error_for_status()?
            }
        };
        Ok(resp.json::<Value>()?)
    }
}
