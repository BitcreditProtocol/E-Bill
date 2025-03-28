use super::Result;
use bcr_ebill_api::service::Error;
use log::info;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Quote;

#[wasm_bindgen]
impl Quote {
    #[wasm_bindgen]
    pub fn new() -> Self {
        Quote
    }

    #[wasm_bindgen(unchecked_return_type = "BitcreditEbillQuote")]
    pub async fn get(&self, id: &str) -> Result<JsValue> {
        info!("return quote called with {id} - not implemented");
        Err(Error::NotFound.into())
    }

    #[wasm_bindgen(unchecked_return_type = "BitcreditEbillQuote")]
    pub async fn accept(&self, id: &str) -> Result<JsValue> {
        info!("accept quote called with {id} - not implemented");
        Err(Error::NotFound.into())
    }
}

impl Default for Quote {
    fn default() -> Self {
        Quote
    }
}
