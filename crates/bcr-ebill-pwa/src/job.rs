use bcr_ebill_api::util::date::now;
use log::{error, info};

use crate::context::get_ctx;

pub fn run_jobs() {
    wasm_bindgen_futures::spawn_local(async {
        futures::join!(
            run_check_bill_payment_job(),
            run_check_bill_offer_to_sell_payment_job(),
            run_check_bill_recourse_payment_job()
        );
        run_check_bill_timeouts().await;
    });
}

async fn run_check_bill_payment_job() {
    info!("Running Check Bill Payment Job");
    if let Err(e) = get_ctx().bill_service.check_bills_payment().await {
        error!("Error while running Check Bill Payment Job: {e}");
    }
    info!("Finished running Check Bill Payment Job");
}

async fn run_check_bill_offer_to_sell_payment_job() {
    info!("Running Check Bill Offer to Sell Payment Job");
    if let Err(e) = get_ctx()
        .bill_service
        .check_bills_offer_to_sell_payment()
        .await
    {
        error!("Error while running Check Bill Offer to Sell Payment Job: {e}");
    }
    info!("Finished running Check Bill Offer to Sell Payment Job");
}

async fn run_check_bill_recourse_payment_job() {
    info!("Running Check Bill Recourse Payment Job");
    if let Err(e) = get_ctx()
        .bill_service
        .check_bills_in_recourse_payment()
        .await
    {
        error!("Error while running Check Bill Recourse Payment Job: {e}");
    }
    info!("Finished running Check Bill Recourse Payment Job");
}

async fn run_check_bill_timeouts() {
    info!("Running Check Bill Timeouts Job");
    let current_time = now().timestamp();
    if let Err(e) = get_ctx()
        .bill_service
        .check_bills_timeouts(current_time as u64)
        .await
    {
        error!("Error while running Check Bill Timeouts Job: {e}");
    }

    info!("Finished running Check Bill Timeouts Job");
}
