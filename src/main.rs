use std::io::{BufRead, BufReader};
use std::net::Ipv6Addr;
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::{env, thread};

use cloudflare::CloudflareClient;
use regex::Regex;
use retry::delay::Exponential;
use retry::retry;
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

mod cloudflare;

fn main() -> anyhow::Result<()> {
    println!("DDNS monitoring service");
    let cf_token = env::var("CF_TOKEN").expect("CF_TOKEN not set");
    let zone_id = env::var("ZONE_ID").expect("ZONE_ID not set");
    let iface = env::args().nth(1).expect("Interface parameter is needed");
    let fqdn = env::args().nth(2).expect("FQDN is required");

    let cf_client = CloudflareClient::new(cf_token, zone_id)?;

    let mut cmd = Command::new("ip")
        .args(["-o", "-6", "monitor", "address", "dev", &iface])
        .stdout(Stdio::piped())
        .spawn()
        .expect("error running ip monitor command");

    let stdout = cmd.stdout.take().expect("error getting stdout from child");
    let stdout = BufReader::new(stdout);

    let mut signals = Signals::new([SIGINT, SIGTERM]).expect("Error creating signal hook");
    thread::spawn(move || {
        for _ in signals.forever() {
            cmd.kill().expect("error killing ip command");
            cmd.wait().expect("error waiting for ip command");
        }
    });

    let re = Regex::new(r"^[0-9]+: \w+\s+inet6 ([a-f0-9:]+)/[0-9]+ scope global \\")
        .expect("failed to parse regex");

    let mut current_ip = None;

    for line in stdout.lines().map_while(Result::ok) {
        println!("Input received:\n{}", line);
        if let Some(ip_str) = re
            .captures(&line)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
        {
            let ip = Ipv6Addr::from_str(ip_str)?;
            println!("Parsed ip: {}", ip);
            if current_ip.is_some_and(|c| c == ip) {
                println!("Nothing to do");
            } else {
                println!("Update ip in cloudflare");
                retry(Exponential::from_millis(100).take(9), || {
                    cf_client.update(&ip, &fqdn)
                })
                .map_err(|e| anyhow::anyhow!(e))?;
                current_ip = Some(ip);
            }
        }
    }

    println!("Closing ddns-update");
    Ok(())
}
