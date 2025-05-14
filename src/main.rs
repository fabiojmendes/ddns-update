use std::{env, io::Write, time::Duration};

use anyhow::bail;
use cloudflare::CloudflareClient;
use futures::{
    TryFutureExt,
    stream::{StreamExt, TryStreamExt},
};
use log::Level;
use netlink_sys::{AsyncSocket, SocketAddr};
use rtnetlink::{
    Handle,
    constants::RTMGRP_IPV6_IFADDR,
    packet_core::NetlinkPayload,
    packet_route::{
        RouteNetlinkMessage,
        address::{AddressAttribute, AddressHeaderFlags, AddressScope},
    },
};

mod cloudflare;

async fn get_link_index(handle: Handle, name: String) -> anyhow::Result<u32> {
    log::info!("Looking for link: {}", name);
    let mut links = handle.link().get().match_name(name).execute();
    if let Some(link) = links.try_next().await? {
        return Ok(link.header.index);
    }
    bail!("No link found")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::builder()
        .format(|buf, record| {
            let priority = match record.level() {
                Level::Trace => 7,
                Level::Debug => 7,
                Level::Info => 6,
                Level::Warn => 4,
                Level::Error => 3,
            };
            writeln!(buf, "<{}>[{}]: {}", priority, record.level(), record.args())
        })
        .init();

    log::info!("DDNS monitoring service");
    log::info!(
        "Version {}, built for {} by {}.",
        built_info::PKG_VERSION,
        built_info::TARGET,
        built_info::RUSTC_VERSION
    );
    if let (Some(version), Some(hash), Some(dirty)) = (
        built_info::GIT_VERSION,
        built_info::GIT_COMMIT_HASH_SHORT,
        built_info::GIT_DIRTY,
    ) {
        log::info!("Git version: {version} ({hash})");
        if dirty {
            log::warn!("Repo was dirty!");
        }
    }
    let cf_token = env::var("CF_TOKEN").expect("CF_TOKEN not set");
    let zone_id = env::var("ZONE_ID").expect("ZONE_ID not set");
    let iface = env::args().nth(1).expect("Interface parameter is needed");
    let domain_name = env::args().nth(2).expect("Domain Name is required");

    let cf_client = CloudflareClient::new(cf_token, zone_id)?;

    // Open the netlink socket
    let (mut connection, handle, mut messages) = rtnetlink::new_connection()?;

    // These flags specify what kinds of broadcast messages we want to listen for.
    let mgroup_flags = RTMGRP_IPV6_IFADDR;

    // A netlink socket address is created with said flags.
    let addr = SocketAddr::new(0, mgroup_flags);
    // Said address is bound so new conenctions and thus
    // new message broadcasts can be received.
    connection
        .socket_mut()
        .socket_mut()
        .bind(&addr)
        .expect("failed to bind");
    tokio::spawn(connection);

    let iface_index = get_link_index(handle, iface).await?;

    let mut current_ip = None;

    while let Some((message, _)) = messages.next().await {
        let payload = message.payload;
        if let NetlinkPayload::InnerMessage(RouteNetlinkMessage::NewAddress(addr_msg)) = payload {
            log::debug!("Header: {:?}", addr_msg.header);
            log::debug!("Attributes: {:?}\n\n", addr_msg.attributes);

            if addr_msg.header.index != iface_index
                || addr_msg.header.scope != AddressScope::Universe
                || addr_msg
                    .header
                    .flags
                    .contains(AddressHeaderFlags::Tentative)
            {
                log::debug!("Discard...");
                continue;
            }

            let maybe_ip = addr_msg.attributes.into_iter().find_map(|attr| {
                if let AddressAttribute::Address(addr) = attr {
                    Some(addr)
                } else {
                    None
                }
            });
            if let Some(ip) = maybe_ip {
                if current_ip.is_some_and(|c| c == ip) {
                    log::debug!("No ip change detected")
                } else {
                    log::info!("New ip {}", ip);
                    let hostname = hostname::get()?;
                    let fqdn = format!("{}.{}", hostname.to_string_lossy(), domain_name);
                    let json = tryhard::retry_fn(|| {
                        cf_client
                            .update(&ip, &fqdn)
                            .inspect_err(|e| log::warn!("{:?}", e))
                    })
                    .retries(10)
                    .exponential_backoff(Duration::from_millis(50))
                    .await?;
                    log::info!("Body: {}", serde_json::to_string_pretty(&json)?);
                    current_ip = Some(ip);
                }
            }
        } else {
            log::debug!("Payload not recognized: {:?}", payload);
        };
    }
    Ok(())
}

pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}
