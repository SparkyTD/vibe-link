use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};
use rosc::{OscMessage, OscPacket, OscType};
use std::sync::mpsc::{channel, Receiver, Sender};
use tokio::net::UdpSocket;
use tokio::sync::mpsc::{channel as tokio_channel, Receiver as TokioReceiver, Sender as TokioSender};
use tokio::sync::Notify;
use wildmatch::WildMatch;

#[allow(unused)]
pub struct OscServer {
    pub data_rx: Receiver<OscFloatData>,
    pub pattern_tx: TokioSender<WildMatch>,

    port_update_counter: Arc<AtomicUsize>,
    server_port: Arc<AtomicU16>,
    port_changed: Arc<Notify>,
    found_addresses: Arc<Mutex<HashSet<String>>>,
}

impl OscServer {
    pub fn new(port: u16) -> Self {
        let (data_tx, data_rx) = channel::<OscFloatData>();
        let (pattern_tx, pattern_rx) = tokio_channel::<WildMatch>(1);

        let found_addresses = Arc::new(Mutex::new(HashSet::new()));
        let port_changed = Arc::new(Notify::new());
        let server_port = Arc::new(AtomicU16::new(port));

        let found_addresses_clone = found_addresses.clone();
        let port_changed_clone = port_changed.clone();
        let server_port_clone = server_port.clone();
        tokio::spawn(async move {
            OscServer::osc_thread(data_tx, pattern_rx, found_addresses_clone, port_changed_clone, server_port_clone).await
        });

        Self {
            data_rx,
            pattern_tx,
            found_addresses,
            server_port,
            port_changed,

            port_update_counter: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn set_port(&mut self, port: u16) {
        let update_ticket = self.port_update_counter.fetch_add(1, Ordering::SeqCst) + 1;
        let port_update_counter = self.port_update_counter.clone();
        let server_port = self.server_port.clone();
        let port_changed = self.port_changed.clone();

        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1000));

            let update_index = port_update_counter.load(Ordering::SeqCst);
            if update_index != update_ticket {
                return;
            }

            let current_port = server_port.load(Ordering::SeqCst);
            if current_port == port {
                return;
            }
            server_port.store(port, Ordering::Relaxed);
            port_changed.notify_waiters();
        });
    }

    async fn osc_thread(tx: Sender<OscFloatData>, mut pattern_rx: TokioReceiver<WildMatch>, found_addresses: Arc<Mutex<HashSet<String>>>, port_changed: Arc<Notify>, port: Arc<AtomicU16>) -> anyhow::Result<()> {
        loop {
            let port = port.load(Ordering::SeqCst);
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

                    _ = port_changed.notified() => {
                        break;
                    }
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