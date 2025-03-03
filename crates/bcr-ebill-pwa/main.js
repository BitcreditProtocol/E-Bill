import * as wasm from '../pkg/bcr_ebill_pwa.js';
async function start() {
    let config = {
        bitcoin_network: "mainnet",
        // nostr_relay: "wss://bitcr-cloud-run-04-550030097098.europe-west1.run.app".to_string(),
        nostr_relay: "wss://bitcr-cloud-run-03-550030097098.europe-west1.run.app",
        // surreal_db_connection: "ws://localhost:8800",
        surreal_db_connection: "indxdb://default",
        data_dir: ".",
        job_runner_initial_delay_seconds: 1,
        job_runner_check_interval_seconds: 600,
    };
    await wasm.default();
    await wasm.initialize_api(config);

    let notificationApi = wasm.Api.notification();
    let contactApi = wasm.Api.contact();
    let identityApi = wasm.Api.identity();

    try {
        let identity = await identityApi.return_identity();
        console.log("local identity:", identity);
    } catch(err) {
        console.log("No local identity found - creating..");
        await identityApi.create_identity({
            name: "Johanna Smith",
            email: "jsmith@example.com"
        });
    }

    await notificationApi.subscribe((evt) => {
        console.log("Received event in JS: ", evt);
    });

    await contactApi.get_contact_for_node_id();

    let current_identity = await identityApi.active();
    console.log(current_identity);

    try {
        await identityApi.switch({ t: 1, node_id: "test" });
    } catch(err) {
        console.error("switching identity failed: ", err);
    }

    await wasm.Api.get_bills();
}

await start();
