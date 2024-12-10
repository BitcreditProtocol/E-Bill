use crate::dht::Client;
use anyhow::Result;
use borsh::to_vec;
use log::{error, info};
use std::io::BufRead;
use tokio::sync::broadcast;

pub async fn run_terminal_client(
    mut shutdown_dht_client_receiver: broadcast::Receiver<bool>,
    mut dht_client: Client,
) -> Result<()> {
    // We need to use blocking stdin, because tokio's async stdin isn't meant for interactive
    // use-cases and will block forever on finishing the program
    let (stdin_tx, mut stdin_rx) = tokio::sync::mpsc::channel(100);
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(line) => {
                    let line = line.trim().to_string();
                    if !line.is_empty() {
                        if let Err(e) = stdin_tx.blocking_send(line) {
                            error!("Error handling stdin: {e}");
                        }
                    }
                }
                Err(e) => {
                    error!("Error reading line from stdin: {e}");
                }
            }
        }
    });

    loop {
        tokio::select! {
            line = stdin_rx.recv() => {
                if let Some(next_line) = line {
                    handle_input_line(&mut dht_client, next_line).await
                }
            },
            _ = shutdown_dht_client_receiver.recv() => {
                info!("Shutting down terminal client...");
                break;
            }
        }
    }
    Ok(())
}

async fn handle_input_line(dht_client: &mut Client, line: String) {
    let mut args = line.split(' ');
    match args.next() {
        Some("START_PROVIDING_COMPANY") => {
            let name: String = {
                match args.next() {
                    Some(name) => String::from(name),
                    None => {
                        error!("Expected name.");
                        return;
                    }
                }
            };
            if let Err(e) = dht_client.start_providing_company(&name).await {
                error!("Could not start providing company {name}: {e}");
            };
        }

        Some("START_PROVIDING_BILL") => {
            let name: String = {
                match args.next() {
                    Some(name) => String::from(name),
                    None => {
                        error!("Expected name.");
                        return;
                    }
                }
            };
            if let Err(e) = dht_client.start_providing_bill(&name).await {
                error!("Could not start providing bill {name}: {e}");
            };
        }

        Some("PUT_RECORD") => {
            let key = {
                match args.next() {
                    Some(key) => String::from(key),
                    None => {
                        error!("Expected key");
                        return;
                    }
                }
            };
            let mut values = vec![];
            for value in args.by_ref() {
                values.push(value.to_owned());
            }

            if values.is_empty() {
                error!("Expected value");
                return;
            }

            match to_vec(&values) {
                Ok(v) => {
                    if let Err(e) = dht_client.put_record(key.clone(), v).await {
                        error!("Could not put record {values:?} to {key}: {e}");
                    }
                }
                Err(e) => {
                    error!("Could not serialize value {values:?} to bytes: {e}");
                }
            };
        }

        Some("SEND_MESSAGE") => {
            let topic = {
                match args.next() {
                    Some(key) => String::from(key),
                    None => {
                        error!("Expected topic");
                        return;
                    }
                }
            };
            let msg = {
                match args.next() {
                    Some(value) => String::from(value),
                    None => {
                        error!("Expected msg");
                        return;
                    }
                }
            };

            if let Err(e) = dht_client
                .send_message(msg.clone().into_bytes(), topic.clone())
                .await
            {
                error!("Could not send message {msg} to {topic}: {e}");
            }
        }

        Some("SUBSCRIBE_TO_COMPANY") => {
            let topic = {
                match args.next() {
                    Some(key) => String::from(key),
                    None => {
                        error!("Expected topic");
                        return;
                    }
                }
            };

            if let Err(e) = dht_client.subscribe_to_company_topic(&topic).await {
                error!("Could not subscribe to topic {topic}: {e}");
            }
        }

        Some("SUBSCRIBE_TO_BILL") => {
            let topic = {
                match args.next() {
                    Some(key) => String::from(key),
                    None => {
                        error!("Expected topic");
                        return;
                    }
                }
            };

            if let Err(e) = dht_client.subscribe_to_bill_topic(&topic).await {
                error!("Could not subscribe to topic {topic}: {e}");
            }
        }

        Some("GET_RECORD") => {
            let key = {
                match args.next() {
                    Some(key) => String::from(key),
                    None => {
                        error!("Expected key");
                        return;
                    }
                }
            };
            if let Err(e) = dht_client.get_record(key.clone()).await {
                error!("Could not get record for {key}: {e}");
            }
        }

        Some("GET_BILL_PROVIDERS") => {
            let key = {
                match args.next() {
                    Some(key) => String::from(key),
                    None => {
                        error!("Expected key");
                        return;
                    }
                }
            };
            if let Err(e) = dht_client.get_bill_providers(&key).await {
                error!("Could not get bill providers for {key}: {e}");
            }
        }

        Some("GET_COMPANY_PROVIDERS") => {
            let key = {
                match args.next() {
                    Some(key) => String::from(key),
                    None => {
                        error!("Expected key");
                        return;
                    }
                }
            };
            if let Err(e) = dht_client.get_company_providers(&key).await {
                error!("Could not get company providers for {key}: {e}");
            }
        }

        _ => {
            error!(
                "expected PUT, SEND_MESSAGE, SUBSCRIBE_TO_BILL, SUBSCRIBE_TO_COMPANY, GET_RECORD, PUT_RECORD, START_PROVIDING_BILL, START_PROVIDING_COMPANY, GET_COMPANY_PROVIDERS or GET_BILL_PROVIDERS."
            );
        }
    }
}
