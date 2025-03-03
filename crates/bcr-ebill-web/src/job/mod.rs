use crate::CONFIG;
use crate::service_context::ServiceContext;
use bcr_ebill_api::util::date::now;
use log::{error, info};
use std::time::Duration;
use tokio::{
    sync::broadcast,
    time::{interval, sleep},
};

pub async fn run(
    service_context: ServiceContext,
    mut shutdown_jobs_client_receiver: broadcast::Receiver<bool>,
) {
    sleep(Duration::from_secs(CONFIG.job_runner_initial_delay_seconds)).await;
    info!(
        "Job runner started after {}s of initial delay, running jobs every {}s...",
        CONFIG.job_runner_initial_delay_seconds, CONFIG.job_runner_check_interval_seconds
    );

    let mut check_interval_tick = interval(Duration::from_secs(
        CONFIG.job_runner_check_interval_seconds,
    ));

    loop {
        tokio::select! {
                _ = check_interval_tick.tick() => {
                    run_jobs(&service_context.clone()).await;
                },
                _ = shutdown_jobs_client_receiver.recv() => {
                    info!("Shutting down job runner...");
                    break;
                }
        }
    }
}

async fn run_jobs(service_context: &ServiceContext) {
    tokio::join!(
        run_check_bill_payment_job(service_context.clone()),
        run_check_bill_offer_to_sell_payment_job(service_context.clone()),
        run_check_bill_recourse_payment_job(service_context.clone())
    );
    // explicitly not added to join! because we want to run this job after
    // all payment jobs are done and avoid any concurrency issues.
    run_check_bill_timeouts(service_context.clone()).await;
}

async fn run_check_bill_payment_job(service_context: ServiceContext) {
    info!("Running Check Bill Payment Job");
    if let Err(e) = service_context.bill_service.check_bills_payment().await {
        error!("Error while running Check Bill Payment Job: {e}");
    }
    info!("Finished running Check Bill Payment Job");
}

async fn run_check_bill_offer_to_sell_payment_job(service_context: ServiceContext) {
    info!("Running Check Bill Offer to Sell Payment Job");
    if let Err(e) = service_context
        .bill_service
        .check_bills_offer_to_sell_payment()
        .await
    {
        error!("Error while running Check Bill Offer to Sell Payment Job: {e}");
    }
    info!("Finished running Check Bill Offer to Sell Payment Job");
}

async fn run_check_bill_recourse_payment_job(service_context: ServiceContext) {
    info!("Running Check Bill Recourse Payment Job");
    if let Err(e) = service_context
        .bill_service
        .check_bills_in_recourse_payment()
        .await
    {
        error!("Error while running Check Bill Recourse Payment Job: {e}");
    }
    info!("Finished running Check Bill Recourse Payment Job");
}

async fn run_check_bill_timeouts(service_context: ServiceContext) {
    info!("Running Check Bill Timeouts Job");
    let current_time = now().timestamp();
    if let Err(e) = service_context
        .bill_service
        .check_bills_timeouts(current_time as u64)
        .await
    {
        error!("Error while running Check Bill Timeouts Job: {e}");
    }

    info!("Finished running Check Bill Timeouts Job");
}
