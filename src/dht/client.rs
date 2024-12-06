use super::behaviour::{
    file_request_for_bill, file_request_for_bill_attachment, file_request_for_bill_keys,
    file_request_for_company_data, file_request_for_company_keys, file_request_for_company_logo,
    file_request_for_company_proof, parse_inbound_file_request, BillAttachmentFileRequest,
    BillFileRequest, BillKeysFileRequest, Command, CompanyDataRequest, CompanyEvent,
    CompanyKeysRequest, CompanyLogoRequest, CompanyProofRequest, Event, FileResponse,
    ParsedInboundFileRequest,
};
use super::{GossipsubEvent, GossipsubEventId, Result};
use crate::constants::{
    BILLS_PREFIX, BILL_PREFIX, COMPANIES_PREFIX, COMPANY_PREFIX, IDENTITY_PREFIX,
};
use crate::persistence::bill::BillStoreApi;
use crate::persistence::company::{
    company_from_bytes, company_keys_from_bytes, company_keys_to_bytes, company_to_bytes,
    CompanyStoreApi,
};
use crate::persistence::identity::IdentityStoreApi;
use crate::service::company_service::{Company, CompanyKeys, CompanyPublicData};
use crate::service::contact_service::IdentityPublicData;
use crate::util;
use crate::util::rsa::{decrypt_bytes_with_private_key, encrypt_bytes_with_public_key};
use borsh::{from_slice, to_vec};
use future::try_join_all;
use futures::channel::mpsc::Receiver;
use futures::channel::{mpsc, oneshot};
use futures::prelude::*;
use libp2p::kad::record::Record;
use libp2p::request_response::ResponseChannel;
use libp2p::PeerId;
use log::{error, info};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::broadcast;

/// The DHT client
#[derive(Clone)]
pub struct Client {
    /// The sender to send events to the event loop
    pub(super) sender: mpsc::Sender<Command>,
    bill_store: Arc<dyn BillStoreApi>,
    company_store: Arc<dyn CompanyStoreApi>,
    identity_store: Arc<dyn IdentityStoreApi>,
}

impl Client {
    pub fn new(
        sender: mpsc::Sender<Command>,
        bill_store: Arc<dyn BillStoreApi>,
        company_store: Arc<dyn CompanyStoreApi>,
        identity_store: Arc<dyn IdentityStoreApi>,
    ) -> Self {
        Self {
            sender,
            bill_store,
            company_store,
            identity_store,
        }
    }

    /// Runs the dht client, listening for incoming events from the event loop
    pub async fn run(
        mut self,
        mut network_events: Receiver<Event>,
        mut shutdown_dht_client_receiver: broadcast::Receiver<bool>,
    ) {
        loop {
            tokio::select! {
                event = network_events.next() => self.handle_event(event.expect("Swarm stream to be infinite.")).await,
                _ = shutdown_dht_client_receiver.recv() => {
                    info!("Shutting down dht client...");
                    break;
                }
            }
        }
    }

    // --------------------------------------------------------------
    // Company-related logic ---------------------------------------
    // --------------------------------------------------------------

    fn company_key(&self, company_id: &str) -> String {
        format!("{COMPANY_PREFIX}{company_id}")
    }

    fn node_request_for_companies(&self, node_id: &str) -> String {
        format!("{COMPANIES_PREFIX}{node_id}")
    }

    /// Gets the list of companies from the network for the local node and gets the companies that
    /// aren't locally available from the network and starts providing them and subscribing to them
    pub async fn check_companies(&mut self) -> Result<()> {
        log::info!("Checking for new companies...");
        let local_peer_id = self.identity_store.get_peer_id().await?;
        let node_request = self.node_request_for_companies(&local_peer_id.to_string());
        let companies_for_node = self.get_record(node_request.clone()).await?.value;
        if companies_for_node.is_empty() {
            return Ok(());
        }
        let companies: Vec<String> = from_slice(&companies_for_node)?;
        let local_companies: Vec<String> = self
            .company_store
            .get_all()
            .await?
            .iter()
            .map(|(id, (_, _))| id.to_string())
            .collect();

        let local_but_not_remote: Vec<String> = local_companies
            .into_iter()
            .filter(|item| !companies.contains(item))
            .collect();

        // Remove the companies, we have locally, but where the network tells us we're not a part
        // of anymore
        for company in local_but_not_remote {
            self.company_store.remove(&company).await?;
            self.stop_providing_company(&company).await?;
            self.unsubscribe_from_company_topic(&company).await?;
        }

        // Add companies thet network tells us we're part of, but we don't have locally
        for company in companies {
            if self.company_store.exists(&company).await {
                continue;
            }
            self.get_company_data_from_the_network(&company, &local_peer_id)
                .await?;
            self.start_providing_company(&company).await?;
            self.subscribe_to_company_topic(&company).await?;
        }

        Ok(())
    }

    async fn get_company_data_from_the_network(
        &mut self,
        company_id: &str,
        local_peer_id: &PeerId,
    ) -> Result<()> {
        let mut providers = self.get_company_providers(company_id).await?;
        providers.remove(local_peer_id);
        if providers.is_empty() {
            error!("Get Company Files: No providers found for {company_id}");
            return Ok(());
        }
        let company = self
            .request_company_data(company_id, local_peer_id, &providers)
            .await?;

        let company_keys = self
            .request_company_keys(company_id, local_peer_id, &providers)
            .await?;

        // like with bills, we don't fail on not being able to fetch files
        let mut logo_file: Option<Vec<u8>> = None;
        if let Some(ref logo) = company.logo_file {
            let file_request =
                file_request_for_company_logo(&local_peer_id.to_string(), company_id, &logo.name);
            match self
                .request_company_file(company_id, file_request, &providers, &logo.hash, &logo.name)
                .await
            {
                Err(e) => {
                    logo_file = None;
                    error!("Could not fetch company logo file for {company_id}: {e}");
                }
                Ok(bytes) => {
                    logo_file = Some(bytes);
                }
            };
        }
        let mut proof_file: Option<Vec<u8>> = None;
        if let Some(ref proof) = company.proof_of_registration_file {
            let file_request =
                file_request_for_company_proof(&local_peer_id.to_string(), company_id, &proof.name);
            match self
                .request_company_file(
                    company_id,
                    file_request,
                    &providers,
                    &proof.hash,
                    &proof.name,
                )
                .await
            {
                Err(e) => {
                    proof_file = None;
                    error!("Could not fetch company proof file for {company_id}: {e}");
                }
                Ok(bytes) => {
                    proof_file = Some(bytes);
                }
            };
        }

        self.company_store.insert(company_id, &company).await?;
        self.company_store
            .save_key_pair(company_id, &company_keys)
            .await?;

        if logo_file.is_some() || proof_file.is_some() {
            if let Some(encrypted_logo_bytes) = logo_file {
                if let Some(ref file) = company.logo_file {
                    self.company_store
                        .save_attached_file(&encrypted_logo_bytes, company_id, &file.name)
                        .await?;
                }
            }

            if let Some(encrypted_proof_bytes) = proof_file {
                if let Some(ref file) = company.proof_of_registration_file {
                    self.company_store
                        .save_attached_file(&encrypted_proof_bytes, company_id, &file.name)
                        .await?;
                }
            }
        }
        Ok(())
    }

    async fn request_company_data(
        &mut self,
        company_id: &str,
        local_peer_id: &PeerId,
        providers: &HashSet<PeerId>,
    ) -> Result<Company> {
        let requests = providers.iter().map(|peer| {
            let mut network_client = self.clone();
            let file_request =
                file_request_for_company_data(&local_peer_id.to_string(), company_id);
            async move {
                network_client
                    .request_file(peer.to_owned(), file_request)
                    .await
            }
            .boxed()
        });

        match futures::future::select_ok(requests).await {
            Err(e) => Err(super::Error::NoProviders(format!(
                "Get Company Data: None of the providers returned the file for {company_id}: {e}",
            ))),
            Ok(file_content) => {
                let company = company_from_bytes(&file_content.0)?;
                Ok(company)
            }
        }
    }

    async fn request_company_keys(
        &mut self,
        company_id: &str,
        local_peer_id: &PeerId,
        providers: &HashSet<PeerId>,
    ) -> Result<CompanyKeys> {
        let requests = providers.iter().map(|peer| {
            let mut network_client = self.clone();
            let file_request =
                file_request_for_company_keys(&local_peer_id.to_string(), company_id);
            async move {
                network_client
                    .request_file(peer.to_owned(), file_request)
                    .await
            }
            .boxed()
        });

        match futures::future::select_ok(requests).await {
            Err(e) => Err(super::Error::NoProviders(format!(
                "Get Company Keys: None of the providers returned the file for {company_id}: {e}",
            ))),
            Ok(file_content) => {
                let encrypted_bytes = file_content.0;
                let identity = self.identity_store.get().await?;
                let decrypted_bytes =
                    decrypt_bytes_with_private_key(&encrypted_bytes, &identity.private_key_pem);
                let company_keys = company_keys_from_bytes(&decrypted_bytes)?;
                Ok(company_keys)
            }
        }
    }

    async fn request_company_file(
        &mut self,
        company_id: &str,
        file_request: String,
        providers: &HashSet<PeerId>,
        hash: &str,
        file_name: &str,
    ) -> Result<Vec<u8>> {
        let requests = providers.iter().map(|peer| {
            let mut network_client = self.clone();
            let file_request_clone = file_request.clone();
            async move {
                network_client
                    .request_file(peer.to_owned(), file_request_clone)
                    .await
            }
            .boxed()
        });

        match futures::future::select_ok(requests).await {
            Err(e) => Err(super::Error::NoProviders(format!(
                "Get Company File: None of the providers returned the file for {company_id}: {e}",
            ))),
            Ok(file_content) => {
                let encrypted_bytes = file_content.0;
                let identity = self.identity_store.get().await?;
                let decrypted_bytes =
                    decrypt_bytes_with_private_key(&encrypted_bytes, &identity.private_key_pem);
                let remote_hash = util::sha256_hash(&decrypted_bytes);
                if hash != remote_hash.as_str() {
                    return Err(super::Error::FileHashesDidNotMatch(format!("Get Company File: Hashes didn't match for company {company_id} and file name {file_name}, remote: {remote_hash}, local: {hash}")));
                }
                let encrypted_bytes =
                    encrypt_bytes_with_public_key(&decrypted_bytes, &identity.public_key_pem);
                Ok(encrypted_bytes)
            }
        }
    }

    /// Subscribes to all locally available companies
    pub async fn subscribe_to_all_companies_topics(&mut self) -> Result<()> {
        let companies = self.company_store.get_all().await?;

        if !companies.is_empty() {
            let tasks = companies.into_iter().map(|(id, (_, _))| {
                let mut self_clone = self.clone();
                async move { self_clone.subscribe_to_bill_topic(&id).await }
            });

            try_join_all(tasks).await?;
        }
        Ok(())
    }

    /// Starts providing all locally available companies
    pub async fn start_providing_companies(&mut self) -> Result<()> {
        let companies = self.company_store.get_all().await?;

        if !companies.is_empty() {
            let tasks = companies.into_iter().map(|(id, (_, _))| {
                let mut self_clone = self.clone();
                async move { self_clone.start_providing_company(&id).await }
            });

            try_join_all(tasks).await?;
        }
        Ok(())
    }

    /// Puts all signatories of every local company to the record of the respective company in the DHT
    pub async fn put_companies_for_signatories(&mut self) -> Result<()> {
        log::info!("Putting signatories for local companies in the DHT");
        let companies = self.company_store.get_all().await?;

        // for each signatory, make one set_record with a list of the companies
        if !companies.is_empty() {
            // collect companies for each unique signatory
            let mut signatory_companies_map: HashMap<String, Vec<String>> = HashMap::new();
            for (company_id, (company, _)) in companies {
                for signatory in company.signatories {
                    signatory_companies_map
                        .entry(signatory.clone())
                        .or_default()
                        .push(company_id.clone());
                }
            }

            // for each signatory, set the collected companies on the DHT
            let tasks = signatory_companies_map
                .into_iter()
                .map(|(signatory, company_ids)| {
                    let mut self_clone = self.clone();
                    async move {
                        self_clone
                            .add_companies_to_dht_for_node(company_ids, &signatory)
                            .await
                    }
                });

            try_join_all(tasks).await?;
        }

        Ok(())
    }

    /// Adds the given list of companies to the DHT for the node_id, if the data from the DHT is
    /// empty, or invalid, we just set the given list, otherwise we merge the unique elements of both
    pub async fn add_companies_to_dht_for_node(
        &mut self,
        company_ids: Vec<String>,
        node_id: &str,
    ) -> Result<()> {
        if company_ids.is_empty() {
            return Ok(());
        }

        let node_request = self.node_request_for_companies(node_id);
        let companies_for_node = self.get_record(node_request.clone()).await?.value;
        if companies_for_node.is_empty() {
            self.put_record(node_request, to_vec(&company_ids)?).await?;
        } else {
            match from_slice::<Vec<String>>(&companies_for_node) {
                Ok(dht_record) => {
                    let mut unique_elements: HashSet<String> = HashSet::new();
                    unique_elements.extend(dht_record.clone().into_iter());
                    unique_elements.extend(company_ids.into_iter());
                    let result: Vec<String> = unique_elements.into_iter().collect();

                    if !dht_record.eq(&result) {
                        self.put_record(node_request.clone(), to_vec(&result)?)
                            .await?;
                    }
                }
                Err(e) => {
                    error!("Could not parse company data in dht for {node_id}: {e}");
                    self.put_record(node_request.clone(), to_vec(&company_ids)?)
                        .await?;
                }
            }
        }

        Ok(())
    }

    /// Removes the given company from the list of companies for the given node - if the data is empty,
    /// or invalid, we don't do anything. Otherwise, we remove the given company id from the DHT value,
    /// if it contains it
    pub async fn remove_company_from_dht_for_node(
        &mut self,
        company_id: &str,
        node_id: &str,
    ) -> Result<()> {
        let node_request = self.node_request_for_companies(node_id);
        let companies_for_node = self.get_record(node_request.clone()).await?.value;
        if !companies_for_node.is_empty() {
            match from_slice::<Vec<String>>(&companies_for_node) {
                Ok(dht_record) => {
                    let mut record_for_saving_in_dht = dht_record.clone();
                    record_for_saving_in_dht.retain(|item| item != company_id);

                    if !dht_record.eq(&record_for_saving_in_dht) {
                        self.put_record(node_request.clone(), to_vec(&record_for_saving_in_dht)?)
                            .await?;
                    }
                }
                Err(e) => {
                    error!("Could not parse company data in dht for {node_id} when trying to remove company {company_id}: {e}");
                }
            }
        }

        Ok(())
    }
    /// Adds the given company to the list of companies for the given node - if the data is empty,
    /// or invalid, we just push our local data. Otherwise, we add the given company id to the DHT,
    /// if it doesn't already contain it
    pub async fn add_company_to_dht_for_node(
        &mut self,
        company_id: &str,
        node_id: &str,
    ) -> Result<()> {
        let node_request = self.node_request_for_companies(node_id);
        let mut record_for_saving_in_dht: Vec<String> = vec![];
        let companies_for_node = self.get_record(node_request.clone()).await?.value;
        if companies_for_node.is_empty() {
            record_for_saving_in_dht.push(company_id.to_owned());
            self.put_record(node_request, to_vec(&record_for_saving_in_dht)?)
                .await?;
        } else {
            match from_slice::<Vec<String>>(&companies_for_node) {
                Ok(dht_record) => {
                    record_for_saving_in_dht = dht_record.clone();
                    if !record_for_saving_in_dht.contains(&company_id.to_string()) {
                        record_for_saving_in_dht.push(company_id.to_owned());
                    }
                    if !dht_record.eq(&record_for_saving_in_dht) {
                        self.put_record(node_request.clone(), to_vec(&record_for_saving_in_dht)?)
                            .await?;
                    }
                }
                Err(e) => {
                    error!("Could not parse company data in dht for {node_id}: {e}");
                    self.put_record(node_request.clone(), to_vec(&vec![company_id.to_owned()])?)
                        .await?;
                }
            }
        }

        Ok(())
    }

    /// Puts the public data of all local companies in the DHT
    pub async fn put_companies_public_data_in_dht(&mut self) -> Result<()> {
        log::info!("Putting local company public data in the DHT");
        let companies = self.company_store.get_all().await?;

        if !companies.is_empty() {
            let tasks = companies.into_iter().map(|(id, (company, company_keys))| {
                let mut self_clone = self.clone();
                async move {
                    self_clone
                        .put_company_public_data_in_dht(CompanyPublicData::from_all(
                            id.clone(),
                            company,
                            company_keys,
                        ))
                        .await
                }
            });

            try_join_all(tasks).await?;
        }

        Ok(())
    }

    /// Puts the public data of company in the DHT
    pub async fn put_company_public_data_in_dht(
        &mut self,
        local_public_data: CompanyPublicData,
    ) -> Result<()> {
        let key = self.company_key(&local_public_data.id);
        let local_json = serde_json::to_string(&local_public_data)?.into_bytes();

        let dht_record = self.get_record(key.clone()).await?.value;
        if dht_record.is_empty() {
            self.put_record(key, local_json).await?;
        } else {
            match serde_json::from_slice::<CompanyPublicData>(&dht_record) {
                Err(e) => {
                    error!(
                        "Could not parse public data in dht for {}: {e}",
                        &local_public_data.id
                    );
                    self.put_record(key.clone(), local_json).await?;
                }
                Ok(dht_public_data) => {
                    if !dht_public_data.eq(&local_public_data) {
                        self.put_record(key.clone(), local_json).await?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Queries the DHT for the public company data for the given company id
    #[allow(dead_code)]
    pub async fn get_company_public_data_from_dht(
        &mut self,
        company_id: &str,
    ) -> Result<Option<CompanyPublicData>> {
        let key = self.company_key(company_id);
        let current_info = self.get_record(key.clone()).await?.value;
        if current_info.is_empty() {
            return Ok(None);
        }
        let company_public_data: CompanyPublicData = serde_json::from_slice(&current_info)?;

        Ok(Some(company_public_data))
    }

    // --------------------------------------------------------------
    // Identity-related logic ---------------------------------------
    // --------------------------------------------------------------

    fn identity_key(&self, peer_id: &str) -> String {
        format!("{IDENTITY_PREFIX}{peer_id}")
    }

    /// Adds the local identity's public data into the DHT
    pub async fn put_identity_public_data_in_dht(&mut self) -> Result<()> {
        if self.identity_store.exists().await {
            let identity = self.identity_store.get_full().await?;
            let identity_data = IdentityPublicData::new(
                identity.identity.clone(),
                identity.peer_id.to_string().clone(),
            );

            let key = self.identity_key(&identity_data.peer_id);
            let current_info = self.get_record(key.clone()).await?.value;
            let mut current_info_string = String::new();
            if !current_info.is_empty() {
                current_info_string = std::str::from_utf8(&current_info)?.to_string();
            }
            let value = serde_json::to_string(&identity_data)?;
            if !current_info_string.eq(&value) {
                self.put_record(key, value.into_bytes()).await?;
            }
        }
        Ok(())
    }

    /// Queries the DHT for the public identity data for the given peer id
    pub async fn get_identity_public_data_from_dht(
        &mut self,
        peer_id: String,
    ) -> Result<IdentityPublicData> {
        let key = self.identity_key(&peer_id);
        let current_info = self.get_record(key.clone()).await?.value;
        let mut identity_public_data: IdentityPublicData = IdentityPublicData::new_empty();
        if !current_info.is_empty() {
            let current_info_string = std::str::from_utf8(&current_info)?.to_string();
            identity_public_data = serde_json::from_str(&current_info_string)?;
        }

        Ok(identity_public_data)
    }

    // ---------------------------------------------------------
    // Bill-related logic --------------------------------------
    // ---------------------------------------------------------

    fn bill_key(&self, bill_name: &str) -> String {
        format!("{BILL_PREFIX}{bill_name}")
    }

    fn node_request_for_bills(&self, node_id: &str) -> String {
        format!("{BILLS_PREFIX}{node_id}")
    }

    /// Checks the bill record for the local node, adding missing bills and starts providing them, if necessary
    pub async fn update_bills_table(&mut self, node_id: String) -> Result<()> {
        let node_request = self.node_request_for_bills(&node_id);

        let list_bills_for_node = self.get_record(node_request.clone()).await?;
        let value = list_bills_for_node.value;

        if !value.is_empty() {
            let record_in_dht: Vec<String> = from_slice(&value)?;
            let mut new_record = record_in_dht.clone();

            let bill_names = self.bill_store.get_bill_names().await?;
            for bill_name in bill_names {
                if !record_in_dht.contains(&bill_name) {
                    new_record.push(bill_name.clone());
                    self.start_providing_bill(&bill_name).await?;
                }
            }
            if !record_in_dht.eq(&new_record) {
                self.put_record(node_request.clone(), to_vec(&new_record)?)
                    .await?;
            }
        } else {
            let bill_names = self.bill_store.get_bill_names().await?;
            let mut new_record = Vec::with_capacity(bill_names.len());
            for bill_name in bill_names {
                new_record.push(bill_name.clone());
                self.start_providing_bill(&bill_name).await?;
            }
            if !new_record.is_empty() {
                self.put_record(node_request.clone(), to_vec(&new_record)?)
                    .await?;
            }
        }
        Ok(())
    }

    /// Starts providing all locally available bills
    pub async fn start_providing_bills(&mut self) -> Result<()> {
        let bills = self.bill_store.get_bill_names().await?;
        for bill in bills {
            self.start_providing_bill(&bill).await?;
        }
        Ok(())
    }

    /// Adds the given bill for the given node id - if the data is empty, or invalid, we just push
    /// our data on the DHT, otherwise we check, if we have data the DHT doesn't have and add that
    pub async fn add_bill_to_dht_for_node(&mut self, bill_name: &str, node_id: &str) -> Result<()> {
        let node_request = self.node_request_for_bills(node_id);
        let mut record_for_saving_in_dht: Vec<String> = vec![];
        let list_bills_for_node = self.get_record(node_request.clone()).await?.value;
        if !list_bills_for_node.is_empty() {
            match from_slice::<Vec<String>>(&list_bills_for_node) {
                Ok(dht_record) => {
                    record_for_saving_in_dht = dht_record.clone();
                    if !record_for_saving_in_dht.contains(&bill_name.to_string()) {
                        record_for_saving_in_dht.push(bill_name.to_owned());
                    }
                    if !dht_record.eq(&record_for_saving_in_dht) {
                        self.put_record(node_request.clone(), to_vec(&record_for_saving_in_dht)?)
                            .await?;
                    }
                }
                Err(e) => {
                    error!("Could not parse bill data in dht for {}: {e}", &bill_name);
                    self.put_record(node_request.clone(), to_vec(&vec![bill_name.to_owned()])?)
                        .await?;
                }
            }
        } else {
            record_for_saving_in_dht.push(bill_name.to_owned());
            self.put_record(node_request.clone(), to_vec(&record_for_saving_in_dht)?)
                .await?;
        }

        Ok(())
    }

    /// Checks the DHT for new bills, fetching the bill data, keys and files for them
    pub async fn check_new_bills(&mut self, node_id: String) -> Result<()> {
        let node_request = self.node_request_for_bills(&node_id);
        let list_bills_for_node = self.get_record(node_request.clone()).await?;
        let value = list_bills_for_node.value;

        if !value.is_empty() {
            let bills: Vec<String> = from_slice(&value)?;
            for bill_id in bills {
                if !self.bill_store.bill_exists(&bill_id).await {
                    self.get_bill(&bill_id).await?;

                    self.get_key(&bill_id).await?;

                    if let Ok(chain) = self.bill_store.read_bill_chain_from_file(&bill_id).await {
                        let bill_keys = self.bill_store.read_bill_keys_from_file(&bill_id).await?;
                        let bill = chain.get_first_version_bill(&bill_keys)?;
                        for file in bill.files {
                            if let Err(e) = self.get_bill_attachment(&bill_id, &file.name).await {
                                error!("Could not get bill attachment with file name {} for bill {bill_id}: {e}", file.name);
                            }
                        }
                    }

                    self.sender
                        .send(Command::SubscribeToTopic {
                            topic: bill_id.to_string().clone(),
                        })
                        .await?;
                }
            }
        }
        Ok(())
    }

    /// Requests the data file for the given bill, saving it locally
    pub async fn get_bill(&mut self, name: &str) -> Result<()> {
        let local_peer_id = self.identity_store.get_peer_id().await?;
        let mut providers = self.get_bill_providers(name).await?;
        providers.remove(&local_peer_id);
        if providers.is_empty() {
            error!("Get Bill: No providers found for {name}");
            return Ok(());
        }
        let requests = providers.into_iter().map(|peer| {
            let mut network_client = self.clone();

            let file_request = file_request_for_bill(&local_peer_id.to_string(), name);
            async move { network_client.request_file(peer, file_request).await }.boxed()
        });

        match futures::future::select_ok(requests).await {
            Err(e) => Err(super::Error::NoProviders(format!(
                "Get Bill: None of the providers returned the file for {name}: {e}",
            ))),
            Ok(file_content) => {
                let bytes = file_content.0;
                self.bill_store.write_bill_to_file(name, &bytes).await?;
                Ok(())
            }
        }
    }

    /// Requests the given file for the given bill name, decrypting it, checking it's hash,
    /// encrypting it and saving it once it arrives
    pub async fn get_bill_attachment(&mut self, bill_name: &str, file_name: &str) -> Result<()> {
        // check if there is such a bill and if it contains this file
        let bill_keys = self.bill_store.read_bill_keys_from_file(bill_name).await?;
        let bill = self
            .bill_store
            .read_bill_chain_from_file(bill_name)
            .await?
            .get_first_version_bill(&bill_keys)?;
        let local_hash = match bill.files.iter().find(|file| file.name.eq(file_name)) {
            None => {
                return Err(super::Error::FileNotFoundInBill(format!("Get Bill Attachment: No file found in bill {bill_name} with file name {file_name}")));
            }
            Some(file) => &file.hash,
        };

        let local_peer_id = self.identity_store.get_peer_id().await?;
        let mut providers = self.get_bill_providers(bill_name).await?;
        providers.remove(&local_peer_id);
        if providers.is_empty() {
            error!("Get Bill Attachment: No providers found for {bill_name}");
            return Ok(());
        }

        let requests = providers.into_iter().map(|peer_id| {
            let mut network_client = self.clone();
            let file_request =
                file_request_for_bill_attachment(&local_peer_id.to_string(), bill_name, file_name);
            async move { network_client.request_file(peer_id, file_request).await }.boxed()
        });

        match futures::future::select_ok(requests).await {
            Err(e) => Err(super::Error::NoFileFromProviders(format!(
                "Get Bill Attachment: None of the providers returned the file for {bill_name}: {e}"
            ))),
            Ok(file_content) => {
                let bytes = file_content.0;
                let keys = self.bill_store.read_bill_keys_from_file(bill_name).await?;
                let pr_key = self
                    .identity_store
                    .get_full()
                    .await?
                    .identity
                    .private_key_pem;
                // decrypt file using identity private key and check hash
                let decrypted_with_identity_key =
                    util::rsa::decrypt_bytes_with_private_key(&bytes, &pr_key);
                let decrypted_with_bill_key = util::rsa::decrypt_bytes_with_private_key(
                    &decrypted_with_identity_key,
                    &keys.private_key_pem,
                );
                let remote_hash = util::sha256_hash(&decrypted_with_bill_key);
                if local_hash != remote_hash.as_str() {
                    return Err(super::Error::FileHashesDidNotMatch(format!("Get Bill Attachment: Hashes didn't match for bill {bill_name} and file name {file_name}, remote: {remote_hash}, local: {local_hash}")));
                }
                // encrypt with bill public key and save file locally
                let encrypted = util::rsa::encrypt_bytes_with_public_key(
                    &decrypted_with_bill_key,
                    &keys.public_key_pem,
                );
                self.bill_store
                    .save_attached_file(&encrypted, bill_name, file_name)
                    .await?;
                Ok(())
            }
        }
    }

    /// Requests the keys file for the given bill, decrypting it and saving it locally
    pub async fn get_key(&mut self, name: &str) -> Result<()> {
        let local_peer_id = self.identity_store.get_peer_id().await?;
        let mut providers = self.get_bill_providers(name).await?;
        providers.remove(&local_peer_id);
        if providers.is_empty() {
            error!("Get Bill Attachment: No providers found for {name}");
            return Ok(());
        }
        let requests = providers.into_iter().map(|peer| {
            let mut network_client = self.clone();

            let file_request = file_request_for_bill_keys(&local_peer_id.to_string(), name);
            async move { network_client.request_file(peer, file_request).await }.boxed()
        });

        match futures::future::select_ok(requests).await {
            Err(e) => Err(super::Error::NoFileFromProviders(format!(
                "Get Bill Keys: None of the providers returned the file for {name}: {e}"
            ))),
            Ok(file_content) => {
                let bytes = file_content.0;
                let pr_key = self
                    .identity_store
                    .get_full()
                    .await?
                    .identity
                    .private_key_pem;
                let key_bytes_decrypted = decrypt_bytes_with_private_key(&bytes, &pr_key);

                self.bill_store
                    .write_bill_keys_to_file_as_bytes(name, &key_bytes_decrypted)
                    .await?;
                Ok(())
            }
        }
    }

    /// Puts all participants of every local bill to the record of the respective bill in the DHT
    pub async fn put_bills_for_parties(&mut self) -> Result<()> {
        let bills = self.bill_store.get_bills().await?;

        for bill in bills {
            let bill_keys = self.bill_store.read_bill_keys_from_file(&bill.name).await?;
            let chain = self
                .bill_store
                .read_bill_chain_from_file(&bill.name)
                .await?;
            let nodes = chain.get_all_nodes_from_bill(&bill_keys)?;
            for node in nodes {
                self.add_bill_to_dht_for_node(&bill.name, &node).await?;
            }
        }
        Ok(())
    }

    /// Subscribes to all locally available bills
    pub async fn subscribe_to_all_bills_topics(&mut self) -> Result<()> {
        let bills = self.bill_store.get_bills().await?;

        for bill in bills {
            self.subscribe_to_bill_topic(&bill.name).await?;
        }
        Ok(())
    }

    /// Asks on the topic to receive the current chain of all local bills
    pub async fn receive_updates_for_all_bills_topics(&mut self) -> Result<()> {
        let bills = self.bill_store.get_bills().await?;

        for bill in bills {
            let event = GossipsubEvent::new(GossipsubEventId::CommandGetChain, vec![0; 24]);
            let message = event.to_byte_array()?;

            self.add_message_to_bill_topic(message, &bill.name).await?;
        }
        Ok(())
    }

    // -------------------------------------------------------------
    // Utility Functions for the DHT -------------------------------
    // -------------------------------------------------------------

    async fn add_message_to_topic(&mut self, msg: Vec<u8>, topic: String) -> Result<()> {
        self.send_message(msg, topic).await?;
        Ok(())
    }

    /// Sends the given message to the given bill topic via the event loop
    pub async fn add_message_to_bill_topic(&mut self, msg: Vec<u8>, topic: &str) -> Result<()> {
        let key = self.bill_key(topic);
        self.add_message_to_topic(msg, key).await?;
        Ok(())
    }

    /// Sends the given message to the given company topic via the event loop
    pub async fn add_message_to_company_topic(&mut self, msg: Vec<u8>, topic: &str) -> Result<()> {
        let key = self.company_key(topic);
        self.add_message_to_topic(msg, key).await?;
        Ok(())
    }

    async fn subscribe_to_topic(&mut self, topic: String) -> Result<()> {
        self.sender
            .send(Command::SubscribeToTopic { topic })
            .await?;
        Ok(())
    }

    async fn unsubscribe_from_topic(&mut self, topic: String) -> Result<()> {
        self.sender
            .send(Command::UnsubscribeFromTopic { topic })
            .await?;
        Ok(())
    }

    /// Subscribe to the given bill topic via the event loop
    pub async fn subscribe_to_bill_topic(&mut self, topic: &str) -> Result<()> {
        let key = self.bill_key(topic);
        self.subscribe_to_topic(key).await?;
        Ok(())
    }

    /// Subscribe to the given company topic via the event loop
    pub async fn subscribe_to_company_topic(&mut self, topic: &str) -> Result<()> {
        let key = self.company_key(topic);
        self.subscribe_to_topic(key).await?;
        Ok(())
    }

    /// Unsubscribe from the given company topic via the event loop
    pub async fn unsubscribe_from_company_topic(&mut self, topic: &str) -> Result<()> {
        let key = self.company_key(topic);
        self.unsubscribe_from_topic(key).await?;
        Ok(())
    }

    /// Send the given message to the given topic via the event loop
    pub async fn send_message(&mut self, msg: Vec<u8>, topic: String) -> Result<()> {
        self.sender
            .send(Command::SendMessage { msg, topic })
            .await?;
        Ok(())
    }

    /// Puts the given value into the DHT at the given key via the event loop
    pub async fn put_record(&mut self, key: String, value: Vec<u8>) -> Result<()> {
        self.sender.send(Command::PutRecord { key, value }).await?;
        Ok(())
    }

    /// Gets the record for the given key via the event loop
    pub async fn get_record(&mut self, key: String) -> Result<Record> {
        let (sender, receiver) = oneshot::channel();
        self.sender.send(Command::GetRecord { key, sender }).await?;
        let record = receiver.await?;
        Ok(record)
    }

    async fn start_providing(&mut self, entry: String) -> Result<()> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::StartProviding { entry, sender })
            .await?;
        receiver.await?;
        Ok(())
    }

    async fn stop_providing(&mut self, entry: String) -> Result<()> {
        self.sender.send(Command::StopProviding { entry }).await?;
        Ok(())
    }

    /// Adds the current node to the list of providers for the given bill via the event loop
    pub async fn start_providing_bill(&mut self, bill_name: &str) -> Result<()> {
        let key = self.bill_key(bill_name);
        self.start_providing(key).await?;
        Ok(())
    }

    /// Adds the current node to the list of providers for the given company via the event loop
    pub async fn start_providing_company(&mut self, company_id: &str) -> Result<()> {
        let key = self.company_key(company_id);
        self.start_providing(key).await?;
        Ok(())
    }

    /// Removes the current node from the list of providers for the given company via the event loop
    pub async fn stop_providing_company(&mut self, company_id: &str) -> Result<()> {
        let key = self.company_key(company_id);
        self.stop_providing(key).await?;
        Ok(())
    }

    /// Gets the providers for the given file via the event loop
    async fn get_providers(&mut self, entry: String) -> Result<HashSet<PeerId>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::GetProviders { entry, sender })
            .await
            .expect("Command receiver not to be dropped.");
        let providers = receiver.await?;
        Ok(providers)
    }

    /// Gets the providers for the given bill via the event loop
    pub async fn get_bill_providers(&mut self, bill_id: &str) -> Result<HashSet<PeerId>> {
        let entry = self.bill_key(bill_id);
        let providers = self.get_providers(entry).await?;
        Ok(providers)
    }

    /// Gets the providers for the given company via the event loop
    pub async fn get_company_providers(&mut self, company_id: &str) -> Result<HashSet<PeerId>> {
        let entry = self.company_key(company_id);
        let providers = self.get_providers(entry).await?;
        Ok(providers)
    }

    /// Request the given file via the event loop
    async fn request_file(&mut self, peer: PeerId, file_name: String) -> Result<Vec<u8>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::RequestFile {
                file_name,
                peer,
                sender,
            })
            .await?;
        let res = receiver.await?;
        res.map_err(|e| super::Error::RequestFile(e.to_string()))
    }

    /// Respond with the given file via the event loop
    async fn respond_file(
        &mut self,
        file: Vec<u8>,
        channel: ResponseChannel<FileResponse>,
    ) -> Result<()> {
        self.sender
            .send(Command::RespondFile { file, channel })
            .await?;
        Ok(())
    }

    // -----------------------------------------------------
    // Event Handling --------------------------------------
    // -----------------------------------------------------

    // We check if the node id is part of the signatories, and if so, send the
    // company data
    async fn handle_company_data_file_request(
        &mut self,
        company_id: &str,
        node_id: &str,
        channel: ResponseChannel<FileResponse>,
    ) -> Result<()> {
        let company = self.company_store.get(company_id).await?;
        if company.signatories.iter().any(|v| v == node_id) {
            let bytes = company_to_bytes(&company)?;
            self.respond_file(bytes, channel).await?;
        }
        Ok(())
    }

    // We check if the node id is part of the signatories, and if so, send the
    // company keys encrypted with the node's public key
    async fn handle_company_keys_file_request(
        &mut self,
        company_id: &str,
        node_id: &str,
        channel: ResponseChannel<FileResponse>,
    ) -> Result<()> {
        let company = self.company_store.get(company_id).await?;
        if company.signatories.iter().any(|v| v == node_id) {
            let public_data = self
                .get_identity_public_data_from_dht(node_id.to_owned())
                .await?;
            let public_key = public_data.rsa_public_key_pem;
            let keys = self.company_store.get_key_pair(company_id).await?;
            let bytes = company_keys_to_bytes(&keys)?;
            let file_encrypted = encrypt_bytes_with_public_key(&bytes, &public_key);
            self.respond_file(file_encrypted, channel).await?;
        }
        Ok(())
    }

    // We check if the node id is part of the signatories, and if so,
    // open the logo file, if there is one, decrypt it and send the
    // given file, encrypted with the node's public key
    async fn handle_company_logo_file_request(
        &mut self,
        company_id: &str,
        node_id: &str,
        file_name: &str,
        channel: ResponseChannel<FileResponse>,
    ) -> Result<()> {
        let company = self.company_store.get(company_id).await?;
        if company.signatories.iter().any(|v| v == node_id) {
            if let Some(logo) = company.logo_file {
                if logo.name == *file_name {
                    self.handle_company_file_request_for_file(
                        company_id, node_id, file_name, channel,
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }

    // We check if the node id is part of the signatories, and if so,
    // open the logo file, if there is one, decrypt it and send the
    // given file, encrypted with the node's public key
    async fn handle_company_proof_file_request(
        &mut self,
        company_id: &str,
        node_id: &str,
        file_name: &str,
        channel: ResponseChannel<FileResponse>,
    ) -> Result<()> {
        let company = self.company_store.get(company_id).await?;
        if company.signatories.iter().any(|v| v == node_id) {
            if let Some(proof) = company.proof_of_registration_file {
                if proof.name == *file_name {
                    self.handle_company_file_request_for_file(
                        company_id, node_id, file_name, channel,
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }

    async fn handle_company_file_request_for_file(
        &mut self,
        company_id: &str,
        node_id: &str,
        file_name: &str,
        channel: ResponseChannel<FileResponse>,
    ) -> Result<()> {
        let public_data = self
            .get_identity_public_data_from_dht(node_id.to_owned())
            .await?;
        let public_key = public_data.rsa_public_key_pem;
        let bytes = self
            .company_store
            .open_attached_file(company_id, file_name)
            .await?;
        let identity_private_key = self.identity_store.get().await?.private_key_pem;
        let decrypted_bytes = decrypt_bytes_with_private_key(&bytes, &identity_private_key);
        let file_encrypted = encrypt_bytes_with_public_key(&decrypted_bytes, &public_key);
        self.respond_file(file_encrypted, channel).await?;
        Ok(())
    }

    async fn handle_bill_file_request(
        &mut self,
        bill_name: &str,
        channel: ResponseChannel<FileResponse>,
    ) -> Result<()> {
        let file = self.bill_store.get_bill_as_bytes(bill_name).await?;
        self.respond_file(file, channel).await?;
        Ok(())
    }

    async fn handle_bill_file_request_for_keys(
        &mut self,
        key_name: &str,
        node_id: &str,
        channel: ResponseChannel<FileResponse>,
    ) -> Result<()> {
        let chain = self.bill_store.read_bill_chain_from_file(key_name).await?;
        let bill_keys = self.bill_store.read_bill_keys_from_file(key_name).await?;
        if chain
            .get_all_nodes_from_bill(&bill_keys)?
            .iter()
            .any(|n| n == node_id)
        {
            let data = self
                .get_identity_public_data_from_dht(node_id.to_owned())
                .await?;
            let public_key = data.rsa_public_key_pem;
            let file = self.bill_store.get_bill_keys_as_bytes(key_name).await?;
            let file_encrypted = encrypt_bytes_with_public_key(&file, &public_key);
            self.respond_file(file_encrypted, channel).await?;
        }
        Ok(())
    }

    async fn handle_bill_file_request_for_attachment(
        &mut self,
        bill_name: &str,
        node_id: &str,
        file_name: &str,
        channel: ResponseChannel<FileResponse>,
    ) -> Result<()> {
        let chain = self.bill_store.read_bill_chain_from_file(bill_name).await?;
        let bill_keys = self.bill_store.read_bill_keys_from_file(bill_name).await?;
        if chain
            .get_all_nodes_from_bill(&bill_keys)?
            .iter()
            .any(|n| n == node_id)
        {
            let data = self
                .get_identity_public_data_from_dht(node_id.to_owned())
                .await?;
            let public_key = data.rsa_public_key_pem;
            let file = self
                .bill_store
                .open_attached_file(bill_name, file_name)
                .await?;
            let file_encrypted = encrypt_bytes_with_public_key(&file, &public_key);
            self.respond_file(file_encrypted, channel).await?;
        }
        Ok(())
    }

    // -------------------------------------------------------------
    // Request handling code ---------------------------------------
    // -------------------------------------------------------------
    /// Handles incoming requests
    async fn handle_event(&mut self, event: Event) {
        match event {
            Event::CompanyUpdate {
                event,
                company_id,
                signatory,
            } => {
                match event {
                    CompanyEvent::AddSignatory => {
                        info!("Handling AddSignatory event for {company_id} and {signatory}");
                        // add the signatory that we get from the DHT
                        if let Ok(mut company) = self.company_store.get(&company_id).await {
                            company.signatories.push(signatory.clone());
                            if let Err(e) = self.company_store.update(&company_id, &company).await {
                                error!(
                                    "Could not remove signatory {signatory} from {company_id}: {e}"
                                );
                            }
                        }
                        if let Ok(local_peer_id) = self.identity_store.get_peer_id().await {
                            // If we are removed, remove the local company
                            if signatory == local_peer_id.to_string() {
                                info!("Got from DHT, that we were added to company {company_id} - refreshing data from the network");
                                if let Err(e) = self
                                    .get_company_data_from_the_network(&company_id, &local_peer_id)
                                    .await
                                {
                                    error!("Could not remove local company {company_id}: {e}");
                                }
                                if let Err(e) = self.start_providing_company(&company_id).await {
                                    error!("Could not start providing company {company_id}: {e}");
                                }
                                if let Err(e) = self.subscribe_to_company_topic(&company_id).await {
                                    error!(
                                        "Could not start subscribing to company {company_id}: {e}"
                                    );
                                }
                            }
                        }
                    }
                    CompanyEvent::RemoveSignatory => {
                        info!("Handling RemoveSignatory event for {company_id} and {signatory}");
                        // remove the signatory that we get from the DHT
                        if let Ok(mut company) = self.company_store.get(&company_id).await {
                            company.signatories.retain(|i| i != &signatory);
                            if let Err(e) = self.company_store.update(&company_id, &company).await {
                                error!(
                                    "Could not remove signatory {signatory} from {company_id}: {e}"
                                );
                            }
                        }
                        if let Ok(local_peer_id) = self.identity_store.get_peer_id().await {
                            // If we are removed, remove the local company
                            if signatory == local_peer_id.to_string() {
                                info!("Got from DHT, that we were removed from company {company_id} - deleting company locally");
                                if let Err(e) = self.company_store.remove(&company_id).await {
                                    error!("Could not remove local company {company_id}: {e}");
                                }
                                if let Err(e) = self.stop_providing_company(&company_id).await {
                                    error!("Could not stop providing company {company_id}: {e}");
                                }
                                if let Err(e) =
                                    self.unsubscribe_from_company_topic(&company_id).await
                                {
                                    error!(
                                        "Could not stop subscribing to company {company_id}: {e}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
            Event::InboundRequest { request, channel } => {
                match parse_inbound_file_request(&request) {
                    Err(e) => {
                        error!("Could not handle inbound request {request}: {e}")
                    }
                    Ok(parsed) => {
                        match parsed {
                            ParsedInboundFileRequest::CompanyData(CompanyDataRequest {
                                company_id,
                                node_id,
                            }) => {
                                if let Err(e) = self
                                    .handle_company_data_file_request(
                                        &company_id,
                                        &node_id,
                                        channel,
                                    )
                                    .await
                                {
                                    error!("Could not handle inbound request {request}: {e}")
                                }
                            }
                            ParsedInboundFileRequest::CompanyKeys(CompanyKeysRequest {
                                node_id,
                                company_id,
                            }) => {
                                if let Err(e) = self
                                    .handle_company_keys_file_request(
                                        &company_id,
                                        &node_id,
                                        channel,
                                    )
                                    .await
                                {
                                    error!("Could not handle inbound request {request}: {e}")
                                }
                            }
                            ParsedInboundFileRequest::CompanyLogo(CompanyLogoRequest {
                                node_id,
                                company_id,
                                file_name,
                            }) => {
                                if let Err(e) = self
                                    .handle_company_logo_file_request(
                                        &company_id,
                                        &node_id,
                                        &file_name,
                                        channel,
                                    )
                                    .await
                                {
                                    error!("Could not handle inbound request {request}: {e}")
                                }
                            }
                            ParsedInboundFileRequest::CompanyProof(CompanyProofRequest {
                                node_id,
                                company_id,
                                file_name,
                            }) => {
                                if let Err(e) = self
                                    .handle_company_proof_file_request(
                                        &company_id,
                                        &node_id,
                                        &file_name,
                                        channel,
                                    )
                                    .await
                                {
                                    error!("Could not handle inbound request {request}: {e}")
                                }
                            }
                            // We can send the bill to anyone requesting it, since the content is encrypted
                            // and is useless without the keys
                            ParsedInboundFileRequest::Bill(BillFileRequest { bill_name }) => {
                                if let Err(e) =
                                    self.handle_bill_file_request(&bill_name, channel).await
                                {
                                    error!("Could not handle inbound request {request}: {e}")
                                }
                            }
                            // We check if the requester is part of the bill and if so, we get their
                            // identity from DHT and encrypt the file with their public key
                            ParsedInboundFileRequest::BillKeys(BillKeysFileRequest {
                                node_id,
                                key_name,
                            }) => {
                                if let Err(e) = self
                                    .handle_bill_file_request_for_keys(&key_name, &node_id, channel)
                                    .await
                                {
                                    error!("Could not handle inbound request {request}: {e}")
                                }
                            }
                            // We only send attachments (encrypted with the bill public key) to participants of the bill, encrypted with their public key
                            ParsedInboundFileRequest::BillAttachment(
                                BillAttachmentFileRequest {
                                    node_id,
                                    bill_name,
                                    file_name,
                                },
                            ) => {
                                if let Err(e) = self
                                    .handle_bill_file_request_for_attachment(
                                        &bill_name, &node_id, &file_name, channel,
                                    )
                                    .await
                                {
                                    error!("Could not handle inbound request {request}: {e}")
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        persistence::{
            bill::MockBillStoreApi, company::MockCompanyStoreApi, identity::MockIdentityStoreApi,
        },
        tests::test::{TEST_PRIVATE_KEY, TEST_PUB_KEY},
    };
    use futures::channel::mpsc::{self, Sender};
    use libp2p::kad::record::{Key, Record};
    use std::collections::{HashMap, HashSet};

    fn get_baseline_company_data(id: &str) -> (String, (Company, CompanyKeys)) {
        (
            id.to_string(),
            (
                Company {
                    legal_name: "some_name".to_string(),
                    country_of_registration: "AT".to_string(),
                    city_of_registration: "Vienna".to_string(),
                    postal_address: "some address".to_string(),
                    legal_email: "company@example.com".to_string(),
                    registration_number: "some_number".to_string(),
                    registration_date: "2012-01-01".to_string(),
                    proof_of_registration_file: None,
                    logo_file: None,
                    signatories: vec![],
                },
                CompanyKeys {
                    private_key: TEST_PRIVATE_KEY.to_string(),
                    public_key: TEST_PUB_KEY.to_string(),
                },
            ),
        )
    }

    fn get_client() -> Client {
        let (sender, _) = mpsc::channel(0);
        Client::new(
            sender,
            Arc::new(MockBillStoreApi::new()),
            Arc::new(MockCompanyStoreApi::new()),
            Arc::new(MockIdentityStoreApi::new()),
        )
    }

    fn get_client_chan(sender: Sender<Command>) -> Client {
        Client::new(
            sender,
            Arc::new(MockBillStoreApi::new()),
            Arc::new(MockCompanyStoreApi::new()),
            Arc::new(MockIdentityStoreApi::new()),
        )
    }

    fn get_client_chan_stores(
        mock_bill_storage: MockBillStoreApi,
        mock_company_storage: MockCompanyStoreApi,
        mock_identity_storage: MockIdentityStoreApi,
        sender: Sender<Command>,
    ) -> Client {
        Client::new(
            sender,
            Arc::new(mock_bill_storage),
            Arc::new(mock_company_storage),
            Arc::new(mock_identity_storage),
        )
    }

    fn get_storages() -> (MockBillStoreApi, MockCompanyStoreApi, MockIdentityStoreApi) {
        (
            MockBillStoreApi::new(),
            MockCompanyStoreApi::new(),
            MockIdentityStoreApi::new(),
        )
    }

    #[test]
    fn company_key() {
        assert_eq!(get_client().company_key("id"), "COMPANYid".to_string());
    }

    #[test]
    fn identity_key() {
        assert_eq!(get_client().identity_key("id"), "IDENTITYid".to_string());
    }

    #[test]
    fn bill_key() {
        assert_eq!(get_client().bill_key("id"), "BILLid".to_string());
    }

    #[test]
    fn node_request_for_companies() {
        assert_eq!(
            get_client().node_request_for_companies("nodeid"),
            "COMPANIESnodeid".to_string()
        );
    }

    #[test]
    fn node_request_for_bills() {
        assert_eq!(
            get_client().node_request_for_bills("nodeid"),
            "BILLSnodeid".to_string()
        );
    }

    #[tokio::test]
    async fn get_company_providers() {
        let (sender, mut receiver) = mpsc::channel(10);

        tokio::spawn(async move {
            if let Some(Command::GetProviders { entry, sender }) = receiver.next().await {
                assert_eq!(entry, "COMPANYcompany_id".to_string());

                let mut res = HashSet::new();
                res.insert(PeerId::random());
                sender.send(res).unwrap();
            } else {
                panic!("No command received");
            }
        });
        let result = get_client_chan(sender)
            .get_company_providers("company_id")
            .await
            .unwrap();
        assert!(result.len() == 1);
    }

    #[tokio::test]
    async fn get_bill_providers() {
        let (sender, mut receiver) = mpsc::channel(10);

        tokio::spawn(async move {
            if let Some(Command::GetProviders { entry, sender }) = receiver.next().await {
                assert_eq!(entry, "BILLbill_id".to_string());

                let mut res = HashSet::new();
                res.insert(PeerId::random());
                sender.send(res).unwrap();
            } else {
                panic!("No command received");
            }
        });
        let result = get_client_chan(sender)
            .get_bill_providers("bill_id")
            .await
            .unwrap();
        assert!(result.len() == 1);
    }

    #[tokio::test]
    async fn start_providing_bill() {
        let (sender, mut receiver) = mpsc::channel(10);

        tokio::spawn(async move {
            if let Some(Command::StartProviding { entry, sender }) = receiver.next().await {
                assert_eq!(entry, "BILLbill_id".to_string());
                sender.send(()).unwrap();
            } else {
                panic!("No command received");
            }
        });
        let result = get_client_chan(sender)
            .start_providing_bill("bill_id")
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn start_providing_company() {
        let (sender, mut receiver) = mpsc::channel(10);

        tokio::spawn(async move {
            if let Some(Command::StartProviding { entry, sender }) = receiver.next().await {
                assert_eq!(entry, "COMPANYcompany_id".to_string());
                sender.send(()).unwrap();
            } else {
                panic!("No command received");
            }
        });
        let result = get_client_chan(sender)
            .start_providing_company("company_id")
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn stop_providing_company() {
        let (sender, mut receiver) = mpsc::channel(10);

        tokio::spawn(async move {
            if let Some(Command::StopProviding { entry }) = receiver.next().await {
                assert_eq!(entry, "COMPANYcompany_id".to_string());
            } else {
                panic!("No command received");
            }
        });
        let result = get_client_chan(sender)
            .stop_providing_company("company_id")
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn get_record() {
        let (sender, mut receiver) = mpsc::channel(10);

        tokio::spawn(async move {
            if let Some(Command::GetRecord { key, sender }) = receiver.next().await {
                assert_eq!(key, "key".to_string());
                sender
                    .send(Record::new(Key::new(&"key".to_string()), vec![]))
                    .unwrap();
            } else {
                panic!("No command received");
            }
        });
        let result = get_client_chan(sender).get_record("key".to_string()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().value.is_empty());
    }

    #[tokio::test]
    async fn put_record() {
        let (sender, mut receiver) = mpsc::channel(10);

        tokio::spawn(async move {
            if let Some(Command::PutRecord { key, value }) = receiver.next().await {
                assert_eq!(key, "key".to_string());
                assert!(value.is_empty());
            } else {
                panic!("No command received");
            }
        });
        let result = get_client_chan(sender)
            .put_record("key".to_string(), vec![])
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn subscribe_to_bill_topic() {
        let (sender, mut receiver) = mpsc::channel(10);
        let result = get_client_chan(sender)
            .subscribe_to_bill_topic("bill_id")
            .await;
        assert!(result.is_ok());
        match receiver.next().await.unwrap() {
            Command::SubscribeToTopic { topic } => {
                assert_eq!(topic, "BILLbill_id".to_string());
            }
            _ => panic!("got wrong command"),
        };
    }

    #[tokio::test]
    async fn subscribe_to_company_topic() {
        let (sender, mut receiver) = mpsc::channel(10);
        let result = get_client_chan(sender)
            .subscribe_to_company_topic("company_id")
            .await;
        assert!(result.is_ok());
        match receiver.next().await.unwrap() {
            Command::SubscribeToTopic { topic } => {
                assert_eq!(topic, "COMPANYcompany_id".to_string());
            }
            _ => panic!("got wrong command"),
        };
    }

    #[tokio::test]
    async fn unsubscribe_from_company_topic() {
        let (sender, mut receiver) = mpsc::channel(10);
        let result = get_client_chan(sender)
            .unsubscribe_from_company_topic("company_id")
            .await;
        assert!(result.is_ok());
        match receiver.next().await.unwrap() {
            Command::UnsubscribeFromTopic { topic } => {
                assert_eq!(topic, "COMPANYcompany_id".to_string());
            }
            _ => panic!("got wrong command"),
        };
    }

    #[tokio::test]
    async fn add_message_to_bill_topic() {
        let (sender, mut receiver) = mpsc::channel(10);
        let result = get_client_chan(sender)
            .add_message_to_bill_topic(vec![], "bill_id")
            .await;
        assert!(result.is_ok());
        match receiver.next().await.unwrap() {
            Command::SendMessage { msg, topic } => {
                assert_eq!(topic, "BILLbill_id".to_string());
                assert!(msg.is_empty());
            }
            _ => panic!("got wrong command"),
        };
    }

    #[tokio::test]
    async fn add_message_to_company_topic() {
        let (sender, mut receiver) = mpsc::channel(10);
        let result = get_client_chan(sender)
            .add_message_to_company_topic(vec![], "company_id")
            .await;
        assert!(result.is_ok());
        match receiver.next().await.unwrap() {
            Command::SendMessage { msg, topic } => {
                assert_eq!(topic, "COMPANYcompany_id".to_string());
                assert!(msg.is_empty());
            }
            _ => panic!("got wrong command"),
        };
    }

    #[tokio::test]
    async fn subscribe_to_all_companies_topics() {
        let (sender, receiver) = mpsc::channel(10);
        let (bill_store, mut company_store, identity_store) = get_storages();
        company_store.expect_get_all().returning(|| {
            let mut map = HashMap::new();
            let company_1 = get_baseline_company_data("company_1");
            let company_2 = get_baseline_company_data("company_2");
            map.insert(String::from("company_1"), (company_1.1 .0, company_1.1 .1));
            map.insert(String::from("company_2"), (company_2.1 .0, company_2.1 .1));
            Ok(map)
        });
        let result = get_client_chan_stores(bill_store, company_store, identity_store, sender)
            .subscribe_to_all_companies_topics()
            .await;
        assert!(result.is_ok());
        assert_eq!(receiver.count().await, 2);
    }

    #[tokio::test]
    async fn start_providing_companies_does_nothing_if_no_companies() {
        let (sender, _receiver) = mpsc::channel(10);
        let (bill_store, mut company_store, identity_store) = get_storages();
        company_store
            .expect_get_all()
            .returning(|| Ok(HashMap::new()));
        let result = get_client_chan_stores(bill_store, company_store, identity_store, sender)
            .start_providing_companies()
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn start_providing_companies_calls_start_provide_for_each_company() {
        let (sender, mut receiver) = mpsc::channel(10);
        let (bill_store, mut company_store, identity_store) = get_storages();
        company_store.expect_get_all().returning(|| {
            let mut map = HashMap::new();
            let company_1 = get_baseline_company_data("company_1");
            let company_2 = get_baseline_company_data("company_2");
            map.insert(String::from("company_1"), (company_1.1 .0, company_1.1 .1));
            map.insert(String::from("company_2"), (company_2.1 .0, company_2.1 .1));
            Ok(map)
        });

        tokio::spawn(async move {
            let mut count = 0;
            while let Some(Command::StartProviding { entry: _, sender }) = receiver.next().await {
                sender.send(()).unwrap();
                count += 1;
            }
            assert!(count == 2);
        });

        let result = get_client_chan_stores(bill_store, company_store, identity_store, sender)
            .start_providing_companies()
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn add_companies_to_dht_for_node() {
        let (sender, mut receiver) = mpsc::channel(10);

        tokio::spawn(async move {
            while let Some(event) = receiver.next().await {
                match event {
                    Command::PutRecord { key, value } => {
                        assert_eq!(key, "COMPANIESmy_node_id".to_string());
                        let parsed: Vec<String> = from_slice(&value).unwrap();
                        assert!(parsed.contains(&String::from("company_1")));
                        assert!(parsed.contains(&String::from("company_2")));
                        assert!(parsed.len() == 2);
                    }
                    Command::GetRecord { key, sender } => {
                        assert_eq!(key, "COMPANIESmy_node_id".to_string());
                        sender
                            .send(Record::new(
                                Key::new(&"COMPANIESmy_node_id".to_string()),
                                to_vec(&vec!["company_1".to_string()]).unwrap(),
                            ))
                            .unwrap();
                    }
                    _ => panic!("wrong event"),
                }
            }
        });

        let result = get_client_chan(sender)
            .add_companies_to_dht_for_node(
                vec![String::from("company_1"), String::from("company_2")],
                "my_node_id",
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn add_companies_to_dht_for_node_does_nothing_on_empty_list() {
        let result = get_client()
            .add_companies_to_dht_for_node(vec![], "my_node_id")
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn add_companies_to_dht_for_node_puts_given_ids_if_result_from_dht_empty() {
        let (sender, mut receiver) = mpsc::channel(10);

        tokio::spawn(async move {
            while let Some(event) = receiver.next().await {
                match event {
                    Command::PutRecord { key, value } => {
                        assert_eq!(key, "COMPANIESmy_node_id".to_string());
                        let parsed: Vec<String> = from_slice(&value).unwrap();
                        assert!(parsed.contains(&String::from("company_1")));
                        assert!(parsed.contains(&String::from("company_2")));
                        assert!(parsed.len() == 2);
                    }
                    Command::GetRecord { key, sender } => {
                        assert_eq!(key, "COMPANIESmy_node_id".to_string());
                        let result: Vec<String> = vec![];
                        sender
                            .send(Record::new(
                                Key::new(&"COMPANIESmy_node_id".to_string()),
                                to_vec(&result).unwrap(),
                            ))
                            .unwrap();
                    }
                    _ => panic!("wrong event"),
                }
            }
        });

        let result = get_client_chan(sender)
            .add_companies_to_dht_for_node(
                vec![String::from("company_1"), String::from("company_2")],
                "my_node_id",
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn add_companies_to_dht_for_node_puts_given_ids_if_result_from_dht_is_invalid() {
        let (sender, mut receiver) = mpsc::channel(10);

        tokio::spawn(async move {
            while let Some(event) = receiver.next().await {
                match event {
                    Command::PutRecord { key, value } => {
                        assert_eq!(key, "COMPANIESmy_node_id".to_string());
                        let parsed: Vec<String> = from_slice(&value).unwrap();
                        assert!(parsed.contains(&String::from("company_1")));
                        assert!(parsed.contains(&String::from("company_2")));
                        assert!(parsed.len() == 2);
                    }
                    Command::GetRecord { key, sender } => {
                        assert_eq!(key, "COMPANIESmy_node_id".to_string());
                        let result: String = String::from("hello world"); // this is invalid, since
                                                                          // we expect a vec
                        sender
                            .send(Record::new(
                                Key::new(&"COMPANIESmy_node_id".to_string()),
                                to_vec(&result).unwrap(),
                            ))
                            .unwrap();
                    }
                    _ => panic!("wrong event"),
                }
            }
        });

        let result = get_client_chan(sender)
            .add_companies_to_dht_for_node(
                vec![String::from("company_1"), String::from("company_2")],
                "my_node_id",
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn remove_company_from_dht_for_node() {
        let (sender, mut receiver) = mpsc::channel(10);

        tokio::spawn(async move {
            while let Some(event) = receiver.next().await {
                match event {
                    Command::PutRecord { key, value } => {
                        assert_eq!(key, "COMPANIESmy_node_id".to_string());
                        let parsed: Vec<String> = from_slice(&value).unwrap();
                        assert!(parsed.contains(&String::from("company_1")));
                        assert!(parsed.len() == 1);
                    }
                    Command::GetRecord { key, sender } => {
                        assert_eq!(key, "COMPANIESmy_node_id".to_string());
                        sender
                            .send(Record::new(
                                Key::new(&"COMPANIESmy_node_id".to_string()),
                                to_vec(&vec!["company_1".to_string(), "company_2".to_string()])
                                    .unwrap(),
                            ))
                            .unwrap();
                    }
                    _ => panic!("wrong event"),
                }
            }
        });

        let result = get_client_chan(sender)
            .remove_company_from_dht_for_node("company_2", "my_node_id")
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn add_company_to_dht_for_node() {
        let (sender, mut receiver) = mpsc::channel(10);

        tokio::spawn(async move {
            while let Some(event) = receiver.next().await {
                match event {
                    Command::PutRecord { key, value } => {
                        assert_eq!(key, "COMPANIESmy_node_id".to_string());
                        let parsed: Vec<String> = from_slice(&value).unwrap();
                        assert!(parsed.contains(&String::from("company_1")));
                        assert!(parsed.contains(&String::from("company_2")));
                        assert!(parsed.len() == 2);
                    }
                    Command::GetRecord { key, sender } => {
                        assert_eq!(key, "COMPANIESmy_node_id".to_string());
                        sender
                            .send(Record::new(
                                Key::new(&"COMPANIESmy_node_id".to_string()),
                                to_vec(&vec!["company_2".to_string()]).unwrap(),
                            ))
                            .unwrap();
                    }
                    _ => panic!("wrong event"),
                }
            }
        });

        let result = get_client_chan(sender)
            .add_company_to_dht_for_node("company_1", "my_node_id")
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn add_company_to_dht_for_node_puts_given_id_if_result_from_dht_is_invalid() {
        let (sender, mut receiver) = mpsc::channel(10);

        tokio::spawn(async move {
            while let Some(event) = receiver.next().await {
                match event {
                    Command::PutRecord { key, value } => {
                        assert_eq!(key, "COMPANIESmy_node_id".to_string());
                        let parsed: Vec<String> = from_slice(&value).unwrap();
                        assert!(parsed.contains(&String::from("company_1")));
                        assert!(parsed.len() == 1);
                    }
                    Command::GetRecord { key, sender } => {
                        assert_eq!(key, "COMPANIESmy_node_id".to_string());
                        let result: String = String::from("hello world"); // this is invalid, since
                                                                          // we expect a vec
                        sender
                            .send(Record::new(
                                Key::new(&"COMPANIESmy_node_id".to_string()),
                                to_vec(&result).unwrap(),
                            ))
                            .unwrap();
                    }
                    _ => panic!("wrong event"),
                }
            }
        });

        let result = get_client_chan(sender)
            .add_company_to_dht_for_node("company_1", "my_node_id")
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn add_company_to_dht_for_node_puts_given_id_if_result_from_dht_is_empty() {
        let (sender, mut receiver) = mpsc::channel(10);

        tokio::spawn(async move {
            while let Some(event) = receiver.next().await {
                match event {
                    Command::PutRecord { key, value } => {
                        assert_eq!(key, "COMPANIESmy_node_id".to_string());
                        let parsed: Vec<String> = from_slice(&value).unwrap();
                        assert!(parsed.contains(&String::from("company_1")));
                        assert!(parsed.len() == 1);
                    }
                    Command::GetRecord { key, sender } => {
                        assert_eq!(key, "COMPANIESmy_node_id".to_string());
                        let result: Vec<String> = vec![];
                        sender
                            .send(Record::new(
                                Key::new(&"COMPANIESmy_node_id".to_string()),
                                to_vec(&result).unwrap(),
                            ))
                            .unwrap();
                    }
                    _ => panic!("wrong event"),
                }
            }
        });

        let result = get_client_chan(sender)
            .add_company_to_dht_for_node("company_1", "my_node_id")
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn get_company_public_data_from_dht() {
        let (sender, mut receiver) = mpsc::channel(10);

        tokio::spawn(async move {
            while let Some(event) = receiver.next().await {
                match event {
                    Command::GetRecord { key, sender } => {
                        assert_eq!(key, "COMPANYcompany_1".to_string());
                        let company = get_baseline_company_data("company_1");
                        let data =
                            CompanyPublicData::from_all(company.0, company.1 .0, company.1 .1);
                        sender
                            .send(Record::new(
                                Key::new(&"COMPANYcompany_1".to_string()),
                                serde_json::to_string(&data).unwrap().into_bytes(),
                            ))
                            .unwrap();
                    }
                    _ => panic!("wrong event"),
                }
            }
        });

        let result = get_client_chan(sender)
            .get_company_public_data_from_dht("company_1")
            .await;
        assert!(result.is_ok());
        assert!(result.as_ref().unwrap().is_some());
        assert_eq!(result.as_ref().unwrap().as_ref().unwrap().id, "company_1");
    }

    #[tokio::test]
    async fn put_companies_public_data_in_dht() {
        let (sender, mut receiver) = mpsc::channel(10);
        let (bill_store, mut company_store, identity_store) = get_storages();
        company_store.expect_get_all().returning(|| {
            let mut map = HashMap::new();
            let company_1 = get_baseline_company_data("company_1");
            map.insert(String::from("company_1"), (company_1.1 .0, company_1.1 .1));
            Ok(map)
        });

        tokio::spawn(async move {
            while let Some(event) = receiver.next().await {
                match event {
                    Command::PutRecord { key, value } => {
                        assert_eq!(key, "COMPANYcompany_1".to_string());
                        let parsed: CompanyPublicData = serde_json::from_slice(&value).unwrap();
                        assert_eq!(parsed.id, String::from("company_1"));
                    }
                    Command::GetRecord { key, sender } => {
                        assert_eq!(key, "COMPANYcompany_1".to_string());
                        let result: Vec<u8> = vec![];
                        sender
                            .send(Record::new(
                                Key::new(&"COMPANYcompany_1".to_string()),
                                result,
                            ))
                            .unwrap();
                    }
                    _ => panic!("wrong event"),
                }
            }
        });

        let result = get_client_chan_stores(bill_store, company_store, identity_store, sender)
            .put_companies_public_data_in_dht()
            .await;
        assert!(result.is_ok());
    }
}
