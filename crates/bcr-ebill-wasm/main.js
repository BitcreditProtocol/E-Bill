import * as wasm from '../pkg/index.js';

document.getElementById("fileInput").addEventListener("change", uploadFile);
document.getElementById("notif").addEventListener("click", triggerNotif);
document.getElementById("contact_test").addEventListener("click", triggerContact);
document.getElementById("fetch_temp").addEventListener("click", fetchTempFile);
document.getElementById("fetch_contact_file").addEventListener("click", fetchContactFile);
document.getElementById("switch_identity").addEventListener("click", switchIdentity);

// bill actions
document.getElementById("bill_fetch_detail").addEventListener("click", fetchBillDetail);
document.getElementById("bill_fetch_endorsements").addEventListener("click", fetchBillEndorsements);
document.getElementById("bill_fetch_past_endorsees").addEventListener("click", fetchBillPastEndorsees);
document.getElementById("bill_fetch_past_payments").addEventListener("click", fetchBillPastPayments);
document.getElementById("bill_fetch_bills").addEventListener("click", fetchBillBills);
document.getElementById("bill_balances").addEventListener("click", fetchBillBalances);
document.getElementById("bill_search").addEventListener("click", fetchBillSearch);
document.getElementById("endorse_bill").addEventListener("click", endorseBill);
document.getElementById("req_to_accept_bill").addEventListener("click", requestToAcceptBill);
document.getElementById("req_to_pay_bill").addEventListener("click", requestToPayBill);
document.getElementById("offer_to_sell_bill").addEventListener("click", offerToSellBill);
document.getElementById("req_to_recourse_bill").addEventListener("click", requestToRecourseBill);
document.getElementById("reject_accept").addEventListener("click", rejectAcceptBill);
document.getElementById("reject_pay").addEventListener("click", rejectPayBill);
document.getElementById("reject_buying").addEventListener("click", rejectBuyingBill);
document.getElementById("reject_recourse").addEventListener("click", rejectRecourseBill);
document.getElementById("bill_test").addEventListener("click", triggerBill);
document.getElementById("clear_bill_cache").addEventListener("click", clearBillCache);

async function start() {
  let config = {
    log_level: "debug",
    bitcoin_network: "testnet",
    nostr_relay: "wss://bitcr-cloud-run-04-550030097098.europe-west1.run.app",
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
  } catch (err) {
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

    // add self to contacts
    await contactApi.create({
      t: 0,
      node_id: identity.node_id,
      name: "Self Contact",
      email: "selfcont@example.com",
      postal_address: {
        country: "AT",
        city: "Vienna",
        zip: "1020",
        address: "street 1",
      },
    });
  }
  document.getElementById("identity").innerHTML = identity.node_id;


  await notificationApi.subscribe((evt) => {
    console.log("Received event in JS: ", evt);
  });

  let current_identity = await identityApi.active();
  console.log(current_identity);
  document.getElementById("current_identity").innerHTML = current_identity.node_id;

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
    // add company to contacts
    await contactApi.create({
      t: 1,
      node_id: detail.id,
      name: "Company Contact",
      email: "comcont@example.com",
      postal_address: {
        country: "AT",
        city: "Vienna",
        zip: "1020",
        address: "street 1",
      },
    });
  } else {
    document.getElementById("companies").innerHTML = "node_id: " + companies.companies[0].id;
  }

  // General
  let currencies = await generalApi.currencies();
  console.log("currencies: ", currencies);

  let status = await generalApi.status();
  console.log("status: ", status);

  // Notifications
  let notifications = await notificationApi.list();
  console.log("notifications: ", notifications);
  return { companyApi, generalApi, identityApi, billApi, contactApi, notificationApi };
}

let apis = await start();
let contactApi = apis.contactApi;
let generalApi = apis.generalApi;
let identityApi = apis.identityApi;
let billApi = apis.billApi;
window.billApi = billApi;
window.generalApi = generalApi;
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
    let file_upload_response = await contactApi.upload(uploadedFile);
    console.log("success uploading:", file_upload_response);
    document.getElementById("file_upload_id").value = file_upload_response.file_upload_id;
  } catch (err) {
    console.log("upload error: ", err);
  }
}

async function triggerContact() {
  let node_id = document.getElementById("node_id_bill").value;
  try {
    let contact = await contactApi.detail(node_id);
    console.log("contact:", contact);
    console.log("changing contact");
    await contactApi.edit({
      node_id: node_id,
      name: "Weird Contact",
      postal_address: {
        country: "DE",
        city: "Berlin",
        zip: "10200",
        address: "street 2",
      }
    });
    contact = await contactApi.detail(node_id);
    console.log("contact:", contact);
  } catch (err) {
    console.log("No contact found - creating..");
    let file_upload_id = document.getElementById("file_upload_id").value || undefined;
    await contactApi.create({
      t: 0,
      node_id: node_id,
      name: "Test Contact",
      email: "text@example.com",
      postal_address: {
        country: "AT",
        city: "Vienna",
        zip: "1020",
        address: "street 1",
      },
      avatar_file_upload_id: file_upload_id,
    });
  }
  let contacts = await contactApi.list();
  console.log("contacts: ", contacts);
  let contact = await contactApi.detail(node_id);
  console.log("contact: ", contact);
  document.getElementById("contact_id").value = node_id;
  if (contact.avatar_file) {
    document.getElementById("contact_file_name").value = contact.avatar_file.name;
  }
}

async function triggerBill() {
  let measured = measure(async () => {
    console.log("creating bill");

    const now = new Date();
    const issue_date = now.toISOString().split('T')[0];
    const threeMonthsLater = new Date(now);
    threeMonthsLater.setMonth(now.getMonth() + 3);
    const maturity_date = threeMonthsLater.toISOString().split('T')[0];

    let file_upload_id = document.getElementById("file_upload_id").value || undefined;
    let node_id = document.getElementById("node_id_bill").value;
    let identity = await identityApi.detail();
    let bill = await billApi.issue(
      {
        t: 1,
        country_of_issuing: "AT",
        city_of_issuing: "Vienna",
        issue_date,
        maturity_date,
        payee: identity.node_id,
        drawee: node_id,
        sum: "1500",
        currency: "sat",
        country_of_payment: "UK",
        city_of_payment: "London",
        language: "en-UK",
        file_upload_ids: file_upload_id ? [file_upload_id] : []
      }
    );
    let bill_id = bill.id;
    console.log("created bill with id: ", bill_id);
  });
  await measured();
}

async function triggerNotif() {
  await notificationTriggerApi.trigger_test_msg({ test: "Hello, World" });
}

async function fetchTempFile() {
  let file_upload_id = document.getElementById("file_upload_id").value;
  let temp_file = await generalApi.temp_file(file_upload_id);
  let file_bytes = temp_file.data;
  let arr = new Uint8Array(file_bytes);
  let blob = new Blob([arr], { type: temp_file.content_type });
  let url = URL.createObjectURL(blob);

  console.log("file", temp_file, url, blob);
  document.getElementById("uploaded_file").src = url;
}

async function fetchContactFile() {
  let node_id = document.getElementById("contact_id").value;
  let file_name = document.getElementById("contact_file_name").value;
  let file = await contactApi.file(node_id, file_name);
  let file_bytes = file.data;
  let arr = new Uint8Array(file_bytes);
  let blob = new Blob([arr], { type: file.content_type });
  let url = URL.createObjectURL(blob);

  console.log("file", file, url, blob);
  document.getElementById("attached_file").src = url;
}

async function switchIdentity() {
  let node_id = document.getElementById("node_id_identity").value;
  await identityApi.switch({ t: 1, node_id });
  document.getElementById("current_identity").innerHTML = node_id;
}

async function endorseBill() {
  let bill_id = document.getElementById("endorse_bill_id").value;
  let endorsee = document.getElementById("endorsee_id").value;
  let measured = measure(async () => {
    return await billApi.endorse_bill({ bill_id, endorsee });
  });
  await measured();
}

async function requestToAcceptBill() {
  let bill_id = document.getElementById("endorse_bill_id").value;
  let measured = measure(async () => {
    return await billApi.request_to_accept({ bill_id });
  });
  await measured();
}

async function requestToPayBill() {
  let bill_id = document.getElementById("endorse_bill_id").value;
  let measured = measure(async () => {
    return await billApi.request_to_pay({ bill_id, currency: "sat" });
  });
  await measured();
}

async function offerToSellBill() {
  let bill_id = document.getElementById("endorse_bill_id").value;
  let endorsee = document.getElementById("endorsee_id").value;
  let measured = measure(async () => {
    return await billApi.offer_to_sell({ bill_id, sum: "500", currency: "sat", buyer: endorsee });
  });
  await measured();
}

async function requestToRecourseBill() {
  let bill_id = document.getElementById("endorse_bill_id").value;
  let endorsee = document.getElementById("endorsee_id").value;
  let measured = measure(async () => {
    return await billApi.request_to_recourse_bill_acceptance({ bill_id, recoursee: endorsee });
  });
  await measured();
}

async function rejectAcceptBill() {
  let bill_id = document.getElementById("endorse_bill_id").value;
  let measured = measure(async () => {
    return await billApi.reject_to_accept({ bill_id });
  });
  await measured();
}

async function rejectPayBill() {
  let bill_id = document.getElementById("endorse_bill_id").value;
  let measured = measure(async () => {
    return await billApi.reject_to_pay({ bill_id });
  });
  await measured();
}

async function rejectBuyingBill() {
  let bill_id = document.getElementById("endorse_bill_id").value;
  let measured = measure(async () => {
    return await billApi.reject_to_buy({ bill_id });
  });
  await measured();
}

async function rejectRecourseBill() {
  let bill_id = document.getElementById("endorse_bill_id").value;
  let measured = measure(async () => {
    return await billApi.reject_to_pay_recourse({ bill_id });
  });
  await measured();
}

async function fetchBillDetail() {
  let measured = measure(async () => {
    return await billApi.detail(document.getElementById("bill_id").value);
  });
  await measured();
}

async function fetchBillEndorsements() {
  let measured = measure(async () => {
    return await billApi.endorsements(document.getElementById("bill_id").value);
  });
  await measured();
}

async function fetchBillPastEndorsees() {
  let measured = measure(async () => {
    return await billApi.past_endorsees(document.getElementById("bill_id").value);
  });
  await measured();
}

async function fetchBillPastPayments() {
  let measured = measure(async () => {
    return await billApi.past_payments(document.getElementById("bill_id").value);
  });
  await measured();
}

async function fetchBillBills() {
  let measured = measure(async () => {
    return await billApi.list();
  });
  await measured();
}

async function fetchBillBalances() {
  let measured = measure(async () => {
    return await generalApi.overview("sat");
  });
  await measured();
}

async function fetchBillSearch() {
  let measured = measure(async () => {
    return await billApi.search({ filter: { currency: "sat", role: "All" } });
  });
  await measured();
}

async function clearBillCache() {
  let measured = measure(async () => {
    return await billApi.clear_bill_cache();
  });
  await measured();
}

function measure(promiseFunction) {
  return async function (...args) {
    const startTime = performance.now();
    const result = await promiseFunction(...args);
    const endTime = performance.now();
    const exec_time = (endTime - startTime).toFixed(2);
    console.log(`Execution time: ${exec_time} ms`);

    document.getElementById("bill_execution_time").innerHTML = `${exec_time} ms`;
    document.getElementById("bill_results").innerHTML = "<pre>" + JSON.stringify(result, null, 2) + "</pre>";

    return result;
  };
}
