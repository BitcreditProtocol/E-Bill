use super::{BillAction, BillServiceApi, Result, error::Error, service::BillService};
use crate::util;
use bcr_ebill_core::{
    File,
    bill::{BillKeys, BillType, BitcreditBill},
    blockchain::{
        Blockchain,
        bill::{BillBlockchain, block::BillIssueBlockData},
    },
    contact::IdentityPublicData,
    util::BcrKeys,
};
use bcr_ebill_transport::BillChainEvent;
use log::error;

impl BillService {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn issue_bill(
        &self,
        t: u64,
        country_of_issuing: String,
        city_of_issuing: String,
        issue_date: String,
        maturity_date: String,
        drawee: String,
        payee: String,
        sum: String,
        currency: String,
        country_of_payment: String,
        city_of_payment: String,
        language: String,
        file_upload_id: Option<String>,
        drawer_public_data: IdentityPublicData,
        drawer_keys: BcrKeys,
        timestamp: u64,
    ) -> Result<BitcreditBill> {
        let (sum, bill_type) = self.validate_bill_issue(
            &sum,
            &file_upload_id,
            &issue_date,
            &maturity_date,
            &drawee,
            &payee,
            t,
        )?;

        let (public_data_drawee, public_data_payee) = match bill_type {
            // Drawer is payee
            BillType::SelfDrafted => {
                let public_data_drawee = match self.contact_store.get(&drawee).await {
                    Ok(Some(drawee)) => drawee.into(),
                    Ok(None) | Err(_) => {
                        return Err(Error::Validation(String::from(
                            "Can not get drawee identity from contacts.",
                        )));
                    }
                };

                let public_data_payee = drawer_public_data.clone();

                (public_data_drawee, public_data_payee)
            }
            // Drawer is drawee
            BillType::PromissoryNote => {
                let public_data_drawee = drawer_public_data.clone();

                let public_data_payee = match self.contact_store.get(&payee).await {
                    Ok(Some(drawee)) => drawee.into(),
                    Ok(None) | Err(_) => {
                        return Err(Error::Validation(String::from(
                            "Can not get payee identity from contacts.",
                        )));
                    }
                };

                (public_data_drawee, public_data_payee)
            }
            // Drawer is neither drawee nor payee
            BillType::ThreeParties => {
                let public_data_drawee = match self.contact_store.get(&drawee).await {
                    Ok(Some(drawee)) => drawee.into(),
                    Ok(None) | Err(_) => {
                        return Err(Error::Validation(String::from(
                            "Can not get drawee identity from contacts.",
                        )));
                    }
                };

                let public_data_payee = match self.contact_store.get(&payee).await {
                    Ok(Some(drawee)) => drawee.into(),
                    Ok(None) | Err(_) => {
                        return Err(Error::Validation(String::from(
                            "Can not get payee identity from contacts.",
                        )));
                    }
                };

                (public_data_drawee, public_data_payee)
            }
        };

        let identity = self.identity_store.get_full().await?;
        let keys = BcrKeys::new();
        let public_key = keys.get_public_key();

        let bill_id = util::sha256_hash(public_key.as_bytes());
        let bill_keys = BillKeys {
            private_key: keys.get_private_key_string(),
            public_key: keys.get_public_key(),
        };

        self.store.save_keys(&bill_id, &bill_keys).await?;

        let mut bill_files: Vec<File> = vec![];
        if let Some(ref upload_id) = file_upload_id {
            let files = self
                .file_upload_store
                .read_temp_upload_files(upload_id)
                .await
                .map_err(|_| Error::NoFileForFileUploadId)?;
            for (file_name, file_bytes) in files {
                bill_files.push(
                    self.encrypt_and_save_uploaded_file(
                        &file_name,
                        &file_bytes,
                        &bill_id,
                        &public_key,
                    )
                    .await?,
                );
            }
        }

        let bill = BitcreditBill {
            id: bill_id.clone(),
            country_of_issuing,
            city_of_issuing,
            currency,
            sum,
            maturity_date,
            issue_date,
            country_of_payment,
            city_of_payment,
            language,
            drawee: public_data_drawee,
            drawer: drawer_public_data.clone(),
            payee: public_data_payee,
            endorsee: None,
            files: bill_files,
        };

        let signing_keys = self.get_bill_signing_keys(&drawer_public_data, &drawer_keys, &identity);
        let chain = BillBlockchain::new(
            &BillIssueBlockData::from(bill.clone(), signing_keys.signatory_identity, timestamp),
            signing_keys.signatory_keys,
            signing_keys.company_keys,
            keys.clone(),
            timestamp,
        )?;

        let block = chain.get_first_block();
        self.blockchain_store.add_block(&bill.id, block).await?;

        self.add_identity_and_company_chain_blocks_for_signed_bill_action(
            &drawer_public_data,
            &bill_id,
            block,
            &identity.key_pair,
            &drawer_keys,
            timestamp,
        )
        .await?;

        // clean up temporary file uploads, if there are any, logging any errors
        if let Some(ref upload_id) = file_upload_id {
            if let Err(e) = self
                .file_upload_store
                .remove_temp_upload_folder(upload_id)
                .await
            {
                error!("Error while cleaning up temporary file uploads for {upload_id}: {e}");
            }
        }

        // send notification and blocks to all required recipients
        if let Err(e) = self
            .notification_service
            .send_bill_is_signed_event(&BillChainEvent::new(&bill, &chain, &bill_keys, true)?)
            .await
        {
            error!("Error propagating bill via Nostr {e}");
        }

        // If we're the drawee, we immediately accept the bill with timestamp increased by 1 sec
        if bill.drawer == bill.drawee {
            self.execute_bill_action(
                &bill_id,
                BillAction::Accept,
                &drawer_public_data,
                &drawer_keys,
                timestamp + 1,
            )
            .await?;
        }

        Ok(bill)
    }
}
