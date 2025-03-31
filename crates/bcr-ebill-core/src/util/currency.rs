use crate::{ValidationError, constants::SAT_TO_BTC_RATE};
use rust_decimal::Decimal;

pub fn parse_sum(sum: &str) -> Result<u64, ValidationError> {
    match sum.parse::<u64>() {
        Ok(num) => Ok(num),
        Err(_) => Err(ValidationError::InvalidSum),
    }
}

pub fn sum_to_string(sum: u64) -> String {
    sum.to_string()
}

pub fn sat_to_btc(val: u64) -> String {
    let conversion_factor = Decimal::new(1, 0) / Decimal::new(SAT_TO_BTC_RATE, 0);
    let sat_dec = Decimal::from(val);
    let btc_dec = sat_dec * conversion_factor;
    btc_dec.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sat_to_btc_test() {
        assert_eq!(sat_to_btc(1000), String::from("0.00001000"));
        assert_eq!(sat_to_btc(10000), String::from("0.00010000"));
        assert_eq!(sat_to_btc(1), String::from("0.00000001"));
    }
}
