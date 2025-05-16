use std::{env, io::Write};

use log::Level;

pub fn init() {
    if env::var("JOURNAL_STREAM").is_ok() {
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
    } else {
        env_logger::init();
    }

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
}

pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}
