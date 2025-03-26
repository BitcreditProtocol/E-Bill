use crate::util;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct TimeApi {
    pub timestamp: u64,
}

impl TimeApi {
    pub async fn get_atomic_time() -> Self {
        let utc_now = util::date::now();
        let timestamp = utc_now.timestamp() as u64;
        TimeApi { timestamp }
    }
}
