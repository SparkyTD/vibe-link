use std::net::UdpSocket;
use rosc::{OscMessage, OscPacket, OscType};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use wildmatch::WildMatch;

pub struct OscServer {
    pub rx: Receiver<f32>,
}

impl OscServer {
    pub fn new(port: u16) -> Self {
        let (tx, rx) = channel::<f32>();

        thread::spawn(move || {
            OscServer::osc_thread(tx, port);
        });

        Self {
            rx
        }
    }

    fn osc_thread(tx: Sender<f32>, port: u16) {
        let socket = UdpSocket::bind(("0.0.0.0", port)).unwrap();
        let pattern = WildMatch::new("/avatar/parameters/VF9_spsll_SPSLL_Socket_Ring_*");

        let mut buffer = [0; rosc::decoder::MTU];
        loop {
            if let Ok((_size, _addr)) = socket.recv_from(&mut buffer) {
                let (_, osc_data) = rosc::decoder::decode_udp(&buffer).ok().unwrap();
                if let OscPacket::Message(OscMessage { addr, args }) = &osc_data {
                    if args.is_empty() {
                        continue;
                    }

                    if !pattern.matches(addr) {
                        continue;
                    }

                    if let OscType::Float(val) = args[0] {
                        tx.send(val).unwrap();
                    }
                }
            }
        }
    }

    pub fn try_read_value(&self) -> Option<f32> {
        self.rx.try_recv().ok()
    }
}