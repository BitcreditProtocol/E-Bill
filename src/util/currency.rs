use crate::service::{Error, Result};

pub fn parse_sum(sum: &str) -> Result<u64> {
    match sum.parse::<u64>() {
        Ok(num) => Ok(num),
        Err(_) => Err(Error::Validation(format!("invalid sum: {sum}"))),
    }
}

pub fn sum_to_string(sum: u64) -> String {
    sum.to_string()
}

pub enum Currency {
    Sat,
}

impl Currency {
    // Convert a string to a Currency enum
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "sat" => Ok(Currency::Sat),
            _ => Err(Error::Validation(format!("Unsupported currency: {s}"))),
        }
    }

    // Convert a Currency enum to a string
    pub fn as_str(&self, currency: &str) -> Option<&'static str> {
        match Self::from_str(currency) {
            Ok(Currency::Sat) => Some("sat"),
            _ => None,
        }
    }
}

// From satoshis to BTC
pub fn sat_to_btc(sats: u64) -> f64 {
    sats as f64 / 100_000_000.0
}

// From BTC to satoshis
pub fn btc_to_sat(btc: f64) -> u64 {
    (btc * 100_000_000.0) as u64
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_parse_sum_valid() {
        assert_eq!(parse_sum("100").unwrap(), 100);
    }

    #[test]
    fn test_parse_sum_invalid() {
        assert!(parse_sum("abc").is_err());
    }

    #[test]
    fn test_sum_to_string() {
        assert_eq!(sum_to_string(100), "100");
    }

    #[test]
    fn test_currency_from_str_valid() {
        assert!(matches!(Currency::from_str("sat"), Ok(Currency::Sat)));
    }

    #[test]
    fn test_currency_from_str_invalid() {
        assert!(Currency::from_str("usd").is_err());
    }

    #[test]
    fn test_currency_as_str_valid() {
        let currency = Currency::Sat;
        assert_eq!(currency.as_str("sat").unwrap(), "sat");
    }

    #[test]
    fn test_currency_as_str_invalid() {
        let currency = Currency::Sat;
        assert!(currency.as_str("usd").is_err());
    }

    #[test]
    fn test_sat_to_btc() {
        assert_eq!(sat_to_btc(100_000_000), 1.0);
        assert_eq!(sat_to_btc(50_000_000), 0.5);
    }

    #[test]
    fn test_btc_to_sat() {
        assert_eq!(btc_to_sat(1.0), 100_000_000);
        assert_eq!(btc_to_sat(0.5), 50_000_000);
    }
}
