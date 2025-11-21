use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use rosc::{OscMessage, OscPacket, OscType};
use std::sync::mpsc::{channel, Receiver, Sender};
use tokio::net::UdpSocket;
use tokio::sync::mpsc::{channel as tokio_channel, Receiver as TokioReceiver, Sender as TokioSender};
use wildmatch::WildMatch;

#[allow(unused)]
pub struct OscServer {
    pub data_rx: Receiver<OscFloatData>,
    pub pattern_tx: TokioSender<WildMatch>,
    found_addresses: Arc<Mutex<HashSet<String>>>,
}

impl OscServer {
    pub fn new(port: u16) -> Self {
        let (data_tx, data_rx) = channel::<OscFloatData>();
        let (pattern_tx, pattern_rx) = tokio_channel::<WildMatch>(1);

        let found_addresses = Arc::new(Mutex::new(HashSet::new()));

        let found_addresses_clone = found_addresses.clone();
        tokio::spawn(async move {
            OscServer::osc_thread(data_tx, pattern_rx, found_addresses_clone, port).await
        });

        Self {
            data_rx,
            pattern_tx,
            found_addresses,
        }
    }

    async fn osc_thread(tx: Sender<OscFloatData>, mut pattern_rx: TokioReceiver<WildMatch>, found_addresses: Arc<Mutex<HashSet<String>>>, port: u16) -> anyhow::Result<()> {
        let socket = UdpSocket::bind(("0.0.0.0", port)).await?;
        let mut pattern = WildMatch::new("");

        let mut buffer = [0; rosc::decoder::MTU];
        loop {
            tokio::select! {
                _ = socket.recv_from(&mut buffer) => {
                    let (_, osc_data) = rosc::decoder::decode_udp(&buffer).ok().unwrap();
                    if let OscPacket::Message(OscMessage { addr, args }) = osc_data {
                        if args.is_empty() {
                            continue;
                        }

                        if let OscType::Float(val) = args[0] {
                            let mut found_addresses = found_addresses.lock().expect("Could not lock");
                            found_addresses.insert(addr.to_string());

                            if !pattern.matches(&addr) {
                                continue;
                            }

                            tx.send(OscFloatData {
                                value: val,
                                address: addr,
                            })?;
                        }
                    }
                }
                Some(rx_pattern) = pattern_rx.recv() => {
                    pattern = rx_pattern;
                }
            }
        }
    }

    pub fn try_read_value(&self) -> Option<OscFloatData> {
        self.data_rx.try_recv().ok()
    }

    pub fn set_pattern(&mut self, pattern: WildMatch) {
        let pattern_tx = self.pattern_tx.clone();
        tokio::spawn(async move {
            pattern_tx.send(pattern).await.unwrap();
        });
    }

    #[allow(unused)]
    pub fn get_found_addresses(&self) -> HashSet<String> {
        let found_addresses = self.found_addresses.lock().expect("Could not lock");
        found_addresses.clone()
    }
}

#[derive(Debug, Default)]
pub struct OscFloatData {
    pub address: String,
    pub value: f32,
}