import * as wasm from '../pkg/bcr_ebill_pwa.js';

document.getElementById("fileInput").addEventListener("change", uploadFile);
document.getElementById("notif").addEventListener("click", triggerNotif);

async function start() {
    let config = {
        bitcoin_network: "testnet",
        nostr_relay: "wss://bitcr-cloud-run-04-550030097098.europe-west1.run.app",
        surreal_db_connection: "indxdb://default",
        data_dir: ".",
        job_runner_initial_delay_seconds: 1,
        job_runner_check_interval_seconds: 600,
    };
    await wasm.default();
    await wasm.initialize_api(config);

    let notificationApi = wasm.Api.notification();
    let identityApi = wasm.Api.identity();
    let contactApi = wasm.Api.contact();
    let companyApi = wasm.Api.company();
    let billApi = wasm.Api.bill();
    let generalApi = wasm.Api.general();

    let identity;
    // Identity
    try {
        identity = await identityApi.detail();
        console.log("local identity:", identity);
    } catch(err) {
        console.log("No local identity found - creating..");
        await identityApi.create({
            name: "Johanna Smith",
            email: "jsmith@example.com",
            postal_address: {
                country: "AT",
                city: "Vienna",
                zip: "1020",
                address: "street 1",
            }
        });
        identity = await identityApi.detail();
    }

    // Contact
    let contact_node_id = "039180c169e5f6d7c579cf1cefa37bffd47a2b389c8125601f4068c87bea795943"; 
    try {
        let contact = await contactApi.detail(contact_node_id);
        console.log("contact:", contact);
        console.log("changing contact");
        await contactApi.edit({
            node_id: contact_node_id,
            name: "Weird Contact",
            postal_address: {
                country: "DE",
                city: "Berlin",
                zip: "10200",
                address: "street 2",
            }
        });
        contact = await contactApi.detail(contact_node_id);
        console.log("contact:", contact);
    } catch(err) {
        console.log("No contact found - creating..");
        await contactApi.create({
            t: 0,
            node_id: contact_node_id,
            name: "Test Contact",
            email: "text@example.com",
            postal_address: {
                country: "AT",
                city: "Vienna",
                zip: "1020",
                address: "street 1",
            }
        });
    }
    let contacts = await contactApi.list();
    console.log("contacts: ", contacts);

    await notificationApi.subscribe((evt) => {
        console.log("Received event in JS: ", evt);
    });

    let current_identity = await identityApi.active();
    console.log(current_identity);

    try {
        await identityApi.switch({ t: 1, node_id: "test" });
    } catch(err) {
        console.error("switching identity failed: ", err);
    }

    // Company
    let companies = await companyApi.list();
    console.log("companies:", companies.companies.length, companies);
    if (companies.companies.length == 0) {
        let company = await companyApi.create({
            name: "hayek Ltd",
            email: "test@example.com",
            postal_address: {
                country: "AT",
                city: "Vienna",
                zip: "1020",
                address: "street 1",
            }
        });
        console.log("company: ", company);
        await companyApi.edit({ id: company.id, email: "different@example.com", postal_address: {} });
        let detail = await companyApi.detail(company.id);
        console.log("company detail: ", detail);
        await companyApi.add_signatory({ id: detail.id, signatory_node_id: contact_node_id });
        let signatories = await companyApi.list_signatories(detail.id);
        console.log("signatories: ", signatories);
        await companyApi.remove_signatory({ id: detail.id, signatory_node_id: contact_node_id });
    }

    // Bills
    let light_bills = await billApi.list_light();
    let bills = await billApi.list();
    console.log("bills: ", bills.bills.length, light_bills, bills);
    if (bills.bills.length == 0) {
        let bill = await billApi.issue(
            {
                t: 1,
                country_of_issuing: "AT",
                city_of_issuing: "Vienna",
                issue_date: "2025-01-22",
                maturity_date: "2025-06-22",
                payee: identity.node_id,
                drawee: contact_node_id,
                sum: "1500",
                currency: "sat",
                country_of_payment: "UK",
                city_of_payment: "London",
                language: "en-UK",
                file_upload_id: null
            }
        );
        let bill_id = bill.id;
        console.log("bill id: ", bill_id);
        let detail = await billApi.detail(bill_id);
        console.log("Bill Detail: ", detail);
        console.log("requesting to pay..");
        await billApi.request_to_pay({
            bill_id,
            currency: "sat",
        });
        detail = await billApi.detail(bill_id);
        console.log("Bill Detail: ", detail);
        let num_to_words = await billApi.numbers_to_words_for_sum(bill_id);
        console.log("num to words:", num_to_words);
    }


    // General
    let currencies = await generalApi.currencies();
    console.log("currencies: ", currencies);

    let status = await generalApi.status();
    console.log("status: ", status);

    let overview = await generalApi.overview("sat");
    console.log("overview: ", overview);

    // Notifications
    let notifications = await notificationApi.list();
    console.log("notifications: ", notifications);
    return { contactApi, notificationApi };
}

let apis = await start();
let contactUploadFileApi = apis.contactApi;
let notificationTriggerApi = apis.notificationApi;

async function uploadFile(event) {
    const file = event.target.files[0];
    if (!file) return;

    const name = file.name;
    const extension = name.split('.').pop();
    
    const bytes = await file.arrayBuffer();
    const data = new Uint8Array(bytes);

    const uploadedFile = { name, extension, data };

    console.log("File Name:", uploadedFile.name);
    console.log("File Extension:", uploadedFile.extension);
    console.log("File Bytes:", uploadedFile.data);
    try {
        let file_upload_response = await contactUploadFileApi.upload(uploadedFile);
        console.log("success uploading:", file_upload_response);
    } catch(err) {
        console.log("upload error: ", err);
    }
    
}

async function triggerNotif() {
    await notificationTriggerApi.trigger_msg({ test: "Hello, World" });
}

