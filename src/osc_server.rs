use rosc::{OscMessage, OscPacket, OscType};
use std::sync::mpsc::{channel, Receiver, Sender};
use tokio::net::UdpSocket;
use tokio::sync::mpsc::{channel as tokio_channel, Receiver as TokioReceiver, Sender as TokioSender};
use wildmatch::WildMatch;

pub struct OscServer {
    pub data_rx: Receiver<f32>,
    pub pattern_tx: TokioSender<WildMatch>,
}

impl OscServer {
    pub fn new(port: u16) -> Self {
        let (data_tx, data_rx) = channel::<f32>();
        let (pattern_tx, pattern_rx) = tokio_channel::<WildMatch>(1);

        tokio::spawn(async move {
            OscServer::osc_thread(data_tx, pattern_rx, port).await
        });

        Self {
            data_rx,
            pattern_tx,
        }
    }

    async fn osc_thread(tx: Sender<f32>, mut pattern_rx: TokioReceiver<WildMatch>, port: u16) -> anyhow::Result<()> {
        let socket = UdpSocket::bind(("0.0.0.0", port)).await?;
        let mut pattern = WildMatch::new("");

        let mut buffer = [0; rosc::decoder::MTU];
        loop {
            tokio::select! {
                _ = socket.recv_from(&mut buffer) => {
                    let (_, osc_data) = rosc::decoder::decode_udp(&buffer).ok().unwrap();
                    if let OscPacket::Message(OscMessage { addr, args }) = &osc_data {
                        if args.is_empty() {
                            continue;
                        }

                        if !pattern.matches(addr) {
                            continue;
                        }

                        if let OscType::Float(val) = args[0] {
                            tx.send(val)?;
                        }
                    }
                }
                Some(rx_pattern) = pattern_rx.recv() => {
                    pattern = rx_pattern;
                }
            }
        }
    }

    pub fn try_read_value(&self) -> Option<f32> {
        self.data_rx.try_recv().ok()
    }

    pub fn set_pattern(&mut self, pattern: WildMatch) {
        let pattern_tx = self.pattern_tx.clone();
        tokio::spawn(async move {
            pattern_tx.send(pattern).await.unwrap();
        });
    }
}