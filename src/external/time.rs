use serde::Deserialize;

const URL: &str = "https://api.timezonedb.com/v2.1/get-time-zone?key=RQ6ZFDOXPVLR&format=json&by=zone&zone=Europe/Vienna";

/// Documented at https://timezonedb.com/references/get-time-zone
#[derive(Deserialize, Debug)]
pub struct TimeApi {
    pub timestamp: i64,
}

impl TimeApi {
    pub async fn get_atomic_time() -> Self {
        reqwest::get(URL)
            .await
            .expect("Failed to send request")
            .json()
            .await
            .expect("Failed to read response")
    }
}
