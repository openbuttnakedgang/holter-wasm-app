
use seed::{*, prelude::*};

use crate::device::Desc;

pub async fn load_cfg(desc: Desc) -> String {
    let scheme = fetch_scheme().await;
}

async fn fetch_scheme() -> String {
    let response = fetch("public/scheme.json")
        .await
        .expect("HTTP request failed");

    let user: String = response
        .check_status() // ensure we've got 2xx status
        .expect("status check failed")
        .text()
        .await
        .expect("Failed to des");

    user
}
