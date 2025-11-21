use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::consts::{LOVENSE_SERVICE_UUID, LOVENSE_TX_UUID};
use btleplug::api::{Central as _, CentralEvent, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Manager, Peripheral};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use tokio_stream::StreamExt;
use uuid::Uuid;

macro_rules! error_check {
    ($expression:expr, $tx:expr, $message:expr) => {
        match $expression {
            Ok(val) => val,
            Err(err) => {
                _ = $tx.send(BleMessage::AdapterError(format!("{}: {}", $message, err).into()));
                return
            }
        }
    };
}

macro_rules! some_check {
    ($expression:expr, $tx:expr, $message:expr) => {
        match $expression {
            Some(val) => val,
            None => {
                _ = $tx.send(BleMessage::AdapterError(format!("{}", $message).into()));
                return
            }
        }
    };
}

pub struct BluetoothGattService {
    ble_rx: Option<Receiver<BleMessage>>,
    ble_tx: Option<Sender<BleCommand>>,

    last_speed: u8,
    thread_running: Arc<AtomicBool>,
}

impl BluetoothGattService {
    pub fn new() -> Self {
        let mut result = Self {
            ble_rx: None,
            ble_tx: None,
            last_speed: 0,
            thread_running: Arc::new(AtomicBool::new(false)),
        };

        result.start_ble();
        result
    }

    pub fn start_ble(&mut self) {
        if self.thread_running.load(Ordering::Relaxed) {
            eprintln!("Bluetooth GATT service thread is already running");
            return;
        }

        let (gui_tx, ble_rx) = channel::<BleMessage>();
        let (ble_tx, gui_rx) = channel::<BleCommand>();

        self.ble_tx.replace(ble_tx);
        self.ble_rx.replace(ble_rx);

        let thread_running = self.thread_running.clone();
        thread::spawn(move || {
            thread_running.store(true, Ordering::Relaxed);
            Self::ble_thread(gui_tx, gui_rx);
            thread_running.store(false, Ordering::Relaxed);
        });
    }

    pub fn fetch_ble_message(&mut self) -> Option<BleMessage> {
        if let Some(ble_rx) = &self.ble_rx {
            return ble_rx.try_recv().ok();
        }

        None
    }

    pub fn connect(&mut self, device: &BluetoothGattDevice) -> anyhow::Result<()> {
        if let Some(ble_tx) = &self.ble_tx {
            ble_tx.send(BleCommand::Connect(device.device_address.clone()))?;
            return Ok(());
        }

        Err(anyhow::anyhow!("Missing message channels!"))
    }

    pub fn disconnect(&mut self) -> anyhow::Result<()> {
        if let Some(ble_tx) = &self.ble_tx {
            ble_tx.send(BleCommand::Disconnect)?;
            return Ok(());
        }

        Err(anyhow::anyhow!("Missing message channels!"))
    }

    pub fn send_data(&mut self, data: &[u8]) -> anyhow::Result<()> {
        if let Some(ble_tx) = &self.ble_tx {
            ble_tx.send(BleCommand::SendData(data.to_vec()))?;
            return Ok(());
        }

        Err(anyhow::anyhow!("Missing message channels!"))
    }

    pub fn send_speed(&mut self, speed: u8) -> anyhow::Result<()> {
        if speed == self.last_speed {
            return Ok(());
        }

        self.last_speed = speed;

        self.send_data(format!("Vibrate:{};", speed.clamp(0, 20)).as_bytes())
    }

    fn ble_thread(gui_tx: Sender<BleMessage>, gui_rx: Receiver<BleCommand>) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let manager = error_check!(Manager::new().await, gui_tx, "Failed to create BLE manager");
            let adapters = error_check!(manager.adapters().await, gui_tx, "Failed to get adapters");
            let adapter = some_check!(adapters.into_iter().next(), gui_tx, "No adapters found");

            error_check!(adapter.start_scan(ScanFilter {
                services: vec![Uuid::parse_str(LOVENSE_SERVICE_UUID).unwrap()]
            }).await, gui_tx, "Failed to start scan");

            let _ = gui_tx.send(BleMessage::AdapterInitialized);

            let mut events = error_check!(adapter.events().await, gui_tx, "Failed to get BLE events");

            let tx_clone = gui_tx.clone();
            let tx_clone_2 = gui_tx.clone();
            let adapter_clone = adapter.clone();

            tokio::spawn(async move {
                while let Some(event) = events.next().await {
                    match event {
                        CentralEvent::DeviceDiscovered(id) => {
                            if let Ok(peripheral) = adapter_clone.peripheral(&id).await {
                                if let Ok(Some(props)) = peripheral.properties().await {
                                    // println!("{}: {}", "Props from update".green().bold(), format!("{:?}", props).white());

                                    let mut is_valid = false;
                                    for service in props.services {
                                        if service.to_string() == "455a0001-0023-4bd4-bbd5-a6920e4c5653" {
                                            is_valid = true;
                                        }
                                    }
                                    if !is_valid {
                                        continue;
                                    }

                                    let _ = tx_clone.send(BleMessage::DeviceDiscovered(BluetoothGattDevice {
                                        device_address: props.address.to_string(),
                                        device_name: props.local_name.clone(),
                                    }));
                                }
                            }
                        }
                        /*CentralEvent::DeviceUpdated(id) => {
                            if let Ok(peripheral) = adapter_clone.peripheral(&id).await {
                                if let Ok(Some(props)) = peripheral.properties().await {
                                    // println!("{}: {}", "Props from update".cyan(), format!("{:?}", props).white());
                                }
                            }
                        }*/
                        _ => continue,
                    }
                }
            });

            let mut connected_peripheral: Option<Peripheral> = None;

            loop {
                if let Ok(command) = gui_rx.recv() {
                    match command {
                        BleCommand::Connect(address) => {
                            if let Ok(peripherals) = adapter.peripherals().await {
                                for peripheral in peripherals {
                                    if let Ok(Some(props)) = peripheral.properties().await {
                                        if props.address.to_string() == address {
                                            println!("Connecting...");
                                            _ = tx_clone_2.send(BleMessage::DeviceConnecting(address.clone()));
                                            if let Err(_error) = peripheral.connect().await {
                                                eprintln!("Failed to connect peripheral: {}", _error);
                                            } else {
                                                _ = peripheral.discover_services().await;
                                                connected_peripheral.replace(peripheral);
                                                println!("Connected to {}", address);
                                                _ = tx_clone_2.send(BleMessage::DeviceConnected(address.clone()));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        BleCommand::Disconnect => {
                            if let Some(peripheral) = connected_peripheral.take() {
                                let _ = peripheral.disconnect().await;
                                _ = tx_clone_2.send(BleMessage::DeviceDisconnected(peripheral.address().to_string()));
                            }
                        }
                        BleCommand::SendData(data) => {
                            if let Some(peripheral) = &connected_peripheral {
                                let services = peripheral.services();
                                for service in services {
                                    if service.uuid.to_string() != LOVENSE_SERVICE_UUID {
                                        continue;
                                    }

                                    for characteristic in service.characteristics {
                                        if characteristic.uuid.to_string() != LOVENSE_TX_UUID {
                                            continue;
                                        }

                                        _ = peripheral.write(&characteristic, &data, WriteType::WithoutResponse).await;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
    }
}

#[allow(unused)]
#[derive(Debug, Clone)]
pub enum BleMessage {
    AdapterInitialized,
    AdapterError(String),
    DeviceDiscovered(BluetoothGattDevice),
    DeviceConnecting(String),
    DeviceConnected(String),
    DeviceDisconnected(String),
}

// Commands sent from GUI thread to BLE thread
#[derive(Debug)]
pub enum BleCommand {
    Connect(String), // address
    Disconnect,
    SendData(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct BluetoothGattDevice {
    pub device_address: String,
    pub device_name: Option<String>,
}