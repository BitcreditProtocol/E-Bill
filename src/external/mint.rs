use moksha_core::primitives::{
    BillKeys, CurrencyUnit, PaymentMethod, PostMintQuoteBitcreditResponse,
    PostRequestToMintBitcreditResponse,
};
use moksha_wallet::http::CrossPlatformHttpClient;
use moksha_wallet::localstore::sqlite::SqliteLocalStore;
use moksha_wallet::wallet::Wallet;
use std::{fs, path::PathBuf};
use url::Url;

use crate::service::bill_service::BitcreditEbillQuote;
use crate::{
    bill::{
        quotes::{
            add_bitcredit_quote_and_amount_in_quotes_map, add_bitcredit_token_in_quotes_map,
            add_in_quotes_map, get_quote_from_map, read_quotes_map,
        },
        read_keys_from_bill_file,
    },
    web::data::RequestToMintBitcreditBillPayload,
};

// Usage of tokio::main to spawn a new runtime is necessary here, because Wallet is'nt Send - but
// this logic will be replaced soon
#[tokio::main]
pub async fn accept_mint_bitcredit(
    amount: u64,
    bill_id: String,
    node_id: String,
) -> Result<PostMintQuoteBitcreditResponse, Box<dyn std::error::Error>> {
    let dir = PathBuf::from("./data/wallet".to_string());
    let db_path = dir.join("wallet.db").to_str().ok_or("Invalid path")?.to_string();
    let localstore = SqliteLocalStore::with_path(db_path.clone())
        .await?;

    let mint_url = Url::parse("http://127.0.0.1:3338")?;

    let wallet: Wallet<_, CrossPlatformHttpClient> = Wallet::builder()
        .with_localstore(localstore)
        .build()
        .await?;

    let req = wallet.create_quote_bitcredit(&mint_url, bill_id, node_id, amount);

    Ok(req.await?)
}

// Usage of tokio::main to spawn a new runtime is necessary here, because Wallet is'nt Send - but
// this logic will be replaced soon
#[tokio::main]
pub async fn check_bitcredit_quote(bill_id: &str, node_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from("./data/wallet".to_string());
    let db_path = dir.join("wallet.db").to_str().ok_or("Invalid path")?.to_string();
    let localstore = SqliteLocalStore::with_path(db_path.clone())
        .await?;

    let mint_url = Url::parse("http://127.0.0.1:3338")?;

    let wallet: Wallet<_, CrossPlatformHttpClient> = Wallet::builder()
        .with_localstore(localstore)
        .build()
        .await?;

    let result = wallet
        .check_bitcredit_quote(&mint_url, bill_id.to_owned(), node_id.to_owned())
        .await?;

    let quote = result;

    if !quote.quote.is_empty() {
        add_bitcredit_quote_and_amount_in_quotes_map(quote.clone(), bill_id.to_owned())?;
    }

    Ok(())
}

// Usage of tokio::main to spawn a new runtime is necessary here, because Wallet is'nt Send - but
// this logic will be replaced soon
#[tokio::main]
pub async fn client_accept_bitcredit_quote(bill_id: &String) -> Result<String, Box<dyn std::error::Error>> {
    let dir = PathBuf::from("./data/wallet".to_string());
    let db_path = dir.join("wallet.db").to_str().ok_or("Invalid path")?.to_string();

    let localstore = SqliteLocalStore::with_path(db_path.clone())
        .await?;

    let mint_url = Url::parse("http://127.0.0.1:3338")?;

    let wallet: Wallet<_, CrossPlatformHttpClient> = Wallet::builder()
        .with_localstore(localstore)
        .build()
        .await?;

    let clone_bill_id = bill_id.clone();
    let wallet_keysets = wallet
        .add_mint_keysets_by_id(&mint_url, "cr-sat".to_string(), clone_bill_id)
        .await?;
    let wallet_keyset = wallet_keysets.first().ok_or("No keysets found")?;

    let quote = get_quote_from_map(bill_id)?;
    let quote_id = quote.quote_id.clone();
    let amount = quote.amount;

    let mut token = "".to_string();

    if !quote_id.is_empty() && amount > 0 {
        let result = wallet
            .mint_tokens(
                wallet_keyset,
                &PaymentMethod::Bitcredit,
                amount.into(),
                quote_id,
                CurrencyUnit::CrSat,
            )
            .await?;

        token = result
            .serialize(Option::from(CurrencyUnit::CrSat))?;

        add_bitcredit_token_in_quotes_map(token.clone(), bill_id.clone())?;
    }

    Ok(token)
}

// Usage of tokio::main to spawn a new runtime is necessary here, because Wallet is'nt Send - but
// this logic will be replaced soon
#[tokio::main]
pub async fn request_to_mint_bitcredit(
    payload: RequestToMintBitcreditBillPayload,
) -> Result<PostRequestToMintBitcreditResponse, Box<dyn std::error::Error>> {
    let dir = PathBuf::from("./data/wallet".to_string());
    let db_path = dir.join("wallet.db").to_str().ok_or("Invalid path")?.to_string();
    let localstore = SqliteLocalStore::with_path(db_path.clone())
        .await?;

    let mint_url = Url::parse("http://127.0.0.1:3338")?;

    let wallet: Wallet<_, CrossPlatformHttpClient> = Wallet::builder()
        .with_localstore(localstore)
        .build()
        .await?;

    let bill_keys = read_keys_from_bill_file(&payload.bill_name.clone())?;
    let keys: BillKeys = BillKeys {
        private_key_pem: bill_keys.private_key_pem,
        public_key_pem: bill_keys.public_key_pem,
    };

    let req = wallet.send_request_to_mint_bitcredit(&mint_url, payload.bill_name.clone(), keys);

    let quote: BitcreditEbillQuote = BitcreditEbillQuote {
        bill_id: payload.bill_name.clone(),
        quote_id: "".to_string(),
        amount: 0,
        mint_node_id: payload.mint_node.clone(),
        mint_url: mint_url.to_string().clone(),
        accepted: false,
        token: "".to_string(),
    };
    safe_ebill_quote_locally(quote)?;

    Ok(req.await?)
}

pub fn safe_ebill_quote_locally(quote: BitcreditEbillQuote) -> Result<(), Box<dyn std::error::Error>> {
    let map = read_quotes_map()?;
    if !map.contains_key(&quote.bill_id) {
        add_in_quotes_map(quote)?;
    }
    Ok(())
}

pub async fn init_wallet() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from("./data/wallet".to_string());
    if !dir.exists() {
        fs::create_dir_all(dir.clone())?;
    }
    let db_path = dir.join("wallet.db").to_str().ok_or("Invalid path")?.to_string();

    let _localstore = SqliteLocalStore::with_path(db_path.clone())
        .await?;

    Ok(())
}
