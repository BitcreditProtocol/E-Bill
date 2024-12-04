use crate::constants::QUOTE_MAP_FILE_PATH;
use crate::service::bill_service::BitcreditEbillQuote;
use borsh::{to_vec, BorshDeserialize};
use moksha_core::primitives::CheckBitcreditQuoteResponse;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::io::Error;

pub fn read_quotes_map() -> Result<HashMap<String, BitcreditEbillQuote>, Error> {
    if !Path::new(QUOTE_MAP_FILE_PATH).exists() {
        create_quotes_map()?;
    }
    let data: Vec<u8> = fs::read(QUOTE_MAP_FILE_PATH)?;
    let quotes: HashMap<String, BitcreditEbillQuote> = HashMap::try_from_slice(&data)?;
    Ok(quotes)
}

pub fn create_quotes_map() -> Result<(), Error> {
    let quotes: HashMap<String, BitcreditEbillQuote> = HashMap::new();
    write_quotes_map(quotes)
}

pub fn write_quotes_map(map: HashMap<String, BitcreditEbillQuote>) -> Result<(), Error> {
    let quotes_byte = to_vec(&map)?;
    fs::write(QUOTE_MAP_FILE_PATH, quotes_byte)?;
    Ok(())
}

pub fn add_in_quotes_map(quote: BitcreditEbillQuote) -> Result<(), Error> {
    if !Path::new(QUOTE_MAP_FILE_PATH).exists() {
        create_quotes_map()?;
    }

    let mut quotes: HashMap<String, BitcreditEbillQuote> = read_quotes_map()?;

    quotes.insert(quote.bill_id.clone(), quote);
    write_quotes_map(quotes)
}

pub fn get_quote_from_map(bill_id: &String) -> Result<BitcreditEbillQuote, Error> {
    let quotes = read_quotes_map()?;
    if quotes.contains_key(bill_id) {
        let data = quotes.get(bill_id).unwrap().clone();
        Ok(data)
    } else {
        Ok(BitcreditEbillQuote::new_empty())
    }
}

pub fn add_bitcredit_quote_and_amount_in_quotes_map(
    response: CheckBitcreditQuoteResponse,
    bill_id: String,
) -> Result<(), Error> {
    if !Path::new(QUOTE_MAP_FILE_PATH).exists() {
        create_quotes_map()?;
    }

    let mut quotes: HashMap<String, BitcreditEbillQuote> = read_quotes_map()?;
    let mut quote = get_quote_from_map(&bill_id)?;

    quote.amount = response.amount;
    quote.quote_id = response.quote.clone();

    quotes.remove(&bill_id);
    quotes.insert(bill_id.clone(), quote);
    write_quotes_map(quotes)
}

pub fn add_bitcredit_token_in_quotes_map(token: String, bill_id: String) -> Result<(), Error> {
    if !Path::new(QUOTE_MAP_FILE_PATH).exists() {
        create_quotes_map()?;
    }

    let mut quotes: HashMap<String, BitcreditEbillQuote> = read_quotes_map()?;
    let mut quote = get_quote_from_map(&bill_id)?;

    quote.token = token.clone();

    quotes.remove(&bill_id);
    quotes.insert(bill_id.clone(), quote);
    write_quotes_map(quotes)
}
