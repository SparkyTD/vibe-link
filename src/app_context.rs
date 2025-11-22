use crate::bluetooth::gatt::{BleMessage, BluetoothGattDevice, BluetoothGattService};
use crate::bluetooth::generic::BluetoothGenericService;
use crate::osc_server::{OscFloatData, OscServer};
use crate::remote::receiver::{RemoteControlServer, ServerMessage};
use crate::remote::sender::RemoteControlSender;
use crate::settings::{ControlMode, RemoteMode, Settings};
use crate::speed_filter::SpeedFilter;
use base64::Engine;
use eframe::Frame;
use egui::{CentralPanel, Color32, SidePanel, TopBottomPanel};
use std::time::{Duration, Instant};
use url::Url;
use wildmatch::WildMatch;

pub struct AppContext {
    intensity: u8,
    last_intensity: u8,
    last_max_intensity_perc: u8,
    settings: Settings,
    osc_server: OscServer,
    osc_value: OscFloatData,
    remote_receiver: Option<RemoteControlServer>,
    remote_sender: RemoteControlSender,
    sender_url: Option<String>,
    sender_pairing_code: Option<String>,
    sender_state: RemoteSenderState,
    receiver_state: RemoteReceiverState,
    selected_device: u16,
    gatt_service: BluetoothGattService,
    generic_service: BluetoothGenericService,
    adapter_initialized: bool,
    adapter_error: Option<String>,
    adapter_status: Option<AdapterStatus>,
    found_devices: Vec<DeviceProfile>,
    filter: SpeedFilter,
    last_filter_update: Instant,
    show_advanced_settings: bool,
}

impl AppContext {
    pub fn new() -> Self {
        let settings = Settings::load_or_default().unwrap();

        let mut osc_server = OscServer::new(9001);
        osc_server.set_pattern(WildMatch::new(&settings.osc_path));

        let (remote_server,receiver_state) = match &settings.ngrok_token {
            Some(ngrok_token) => {
                let server = RemoteControlServer::new(&ngrok_token);
                if let ControlMode::Remote(RemoteMode::Receiver) = &settings.mode {
                    server.start().unwrap();
                }
                (Some(server), RemoteReceiverState::NotConnected)
            }
            None => (None, RemoteReceiverState::NoToken)
        };

        Self {
            intensity: 0,
            last_intensity: 0,
            last_max_intensity_perc: 0,
            settings,
            osc_server,
            osc_value: OscFloatData::default(),
            remote_receiver: remote_server,
            remote_sender: RemoteControlSender::new(),
            sender_url: None,
            sender_pairing_code: None,
            sender_state: RemoteSenderState::NotConnected,
            receiver_state,
            selected_device: 0,
            gatt_service: BluetoothGattService::new(),
            generic_service: BluetoothGenericService::new(),
            adapter_initialized: false,
            adapter_error: None,
            adapter_status: None,
            found_devices: vec![DeviceProfile::GenericDevice],
            filter: SpeedFilter::new(0.05),
            last_filter_update: Instant::now(),
            show_advanced_settings: false,
        }
    }

    pub fn send_speed(&mut self, speed: f32) {
        _ = match self.selected_device {
            0 => self.generic_service.send_speed((speed * 7f32) as u8),
            _ => self.gatt_service.send_speed((speed * 20f32) as u8),
        };
    }

    fn connect_to_selected(&mut self) {
    self.gatt_service.disconnect().unwrap();

        let index = self.selected_device as usize;
        let device = self.found_devices.get(index).unwrap();

        match device {
            DeviceProfile::GenericDevice => {
                self.gatt_service.disconnect().unwrap();
                self.settings.last_ble_mac.take();
                self.settings.save().unwrap();
                self.adapter_status.take();
            }
            DeviceProfile::GattDevice(device) => {
                self.gatt_service.connect(device).unwrap();
                self.settings.last_ble_mac.replace(device.device_address.clone());
                self.settings.save().unwrap();
                self.send_speed(0.0f32);
            }
        }
    }

    fn handle_osc(&mut self) {
        if let Some(val) = self.osc_server.try_read_value() {
            self.osc_value = val;
        }

        if self.settings.mode == ControlMode::Osc {
            let delta_time = self.last_filter_update.elapsed().as_secs_f32();
            self.last_filter_update = Instant::now();

            let scaled_value = ((self.osc_value.value - self.settings.osc_range_start) / (self.settings.osc_range_end - self.settings.osc_range_start)).clamp(0.0, 1.0);
            let speed_value = self.filter.update(scaled_value, delta_time).clamp(0.0, 5.0);

            let speed_scale = self.settings.max_intensity_percent as f32 / 100.0;

            self.send_speed(speed_value / 5.0 * speed_scale);
        }
    }

    fn handle_ble(&mut self) {
        while let Some(message) = self.gatt_service.fetch_ble_message() {
            match message {
                BleMessage::AdapterInitialized => self.adapter_initialized = true,
                BleMessage::AdapterError(error) => {
                    self.adapter_initialized = false;
                    self.adapter_error.replace(error);
                }
                BleMessage::DeviceDiscovered(device) => {
                    let address = device.device_address.clone();
                    self.found_devices.push(DeviceProfile::GattDevice(device));

                    if let Some(last_device_mac) = &self.settings.last_ble_mac {
                        if last_device_mac == &address && self.selected_device == 0 {
                            let index = self.found_devices.len() - 1;
                            self.selected_device = index as u16;
                            self.connect_to_selected();
                        }
                    }
                }
                BleMessage::DeviceConnecting(device) => {
                    self.adapter_status.replace(AdapterStatus::Connecting(device));
                }
                BleMessage::DeviceConnected(device) => {
                    self.adapter_status.replace(AdapterStatus::Connected(device));
                }
                BleMessage::DeviceDisconnected(_) => {
                    self.adapter_status.replace(AdapterStatus::NotConnected);
                }
            }
        }
    }

    fn handle_remote_receiver(&mut self) {
        if let Some(remote_receiver) = &mut self.remote_receiver {
            while let Some(message) = remote_receiver.recv_message() {
                match message {
                    ServerMessage::Started { url, token } => {
                        self.sender_url.replace(url);
                        self.sender_pairing_code.replace(token);
                        self.receiver_state = RemoteReceiverState::Connected;
                    }
                    ServerMessage::Stopped => {
                        _ = self.sender_url.take();
                        _ = self.sender_pairing_code.take();
                        self.receiver_state = RemoteReceiverState::NotConnected;
                    }
                    ServerMessage::NewConnection => {
                        self.receiver_state = RemoteReceiverState::Active;
                    }
                    ServerMessage::SpeedReceived { speed } => {
                        let intensity = match self.selected_device {
                            0 => (speed * 7.0) as u8,
                            _ => (speed * 20.0) as u8,
                        };

                        _ = match self.selected_device {
                            0 => self.generic_service.send_speed(intensity),
                            _ => self.gatt_service.send_speed(intensity),
                        };

                        self.intensity = intensity;
                        self.receiver_state = RemoteReceiverState::Active;
                    }
                    ServerMessage::Error { message } => {
                        self.receiver_state = RemoteReceiverState::Error(message);
                    }
                    ServerMessage::Initializing => {
                        self.receiver_state = RemoteReceiverState::Connecting;
                    }
                }
            }
        }
    }
}

impl eframe::App for AppContext {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        // Logic
        self.handle_osc();
        self.handle_ble();
        self.handle_remote_receiver();

        // Draw top bar
        TopBottomPanel::top("title_bar").show(ctx, |ui| {
            ui.style_mut().interaction.selectable_labels = false;

            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.heading("VibeLink");
                    ui.label("by Sparky");
                });

                // Show error and retry button if adapter failed to initialize
                // (e.g. BT disabled on OS level, or no adapter connected)
                if !self.adapter_initialized {
                    ui.horizontal(|ui| {
                        if let Some(error) = &self.adapter_error {
                            ui.colored_label(Color32::RED, format!("Adapter error: {}", error));
                        } else {
                            ui.colored_label(Color32::RED, "Adapter error");
                        }
                        if ui.button("Try again").clicked() {
                            self.gatt_service.start_ble();
                            self.generic_service.start_ble();
                        }
                    });
                    ui.add_space(2.0);
                }
            });
        });

        // Draw intensity slider
        if self.settings.mode != ControlMode::Osc {
            SidePanel::right("side_panel")
                .resizable(false)
                .default_width(0.0)
                .show(ctx, |ui| {
                    ui.style_mut().interaction.selectable_labels = false;

                    let available_height = ui.available_height();
                    ui.horizontal(|ui| {
                        ui.add_space(10.0);
                        ui.vertical(|ui| {
                            ui.add_space(20.0);
                            ui.spacing_mut().slider_width = available_height - 40.0;
                            let slider_max = match (&self.settings.mode, self.selected_device) {
                                (&ControlMode::Remote(RemoteMode::Sender), _) => 20,
                                (_, 0) => 7,
                                (_, _) => 20,
                            };
                            ui.add_enabled(if let ControlMode::Remote(RemoteMode::Receiver) = self.settings.mode { false } else { true },
                                           egui::Slider::new(&mut self.intensity, 0..=slider_max)
                                               .vertical()
                                               .show_value(false)
                                               .trailing_fill(true),
                            );
                            ui.add_space(20.0);
                        });
                        ui.add_space(4.0);
                    });
                });
        }

        // Draw settings panel
        CentralPanel::default().show(ctx, |ui| {
            ui.style_mut().interaction.selectable_labels = false;

            ui.vertical(|ui| {
                // OSD Mode Toggle
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(self.settings.mode != ControlMode::Manual, |ui| {
                        if ui.button("Manual").clicked() {
                            self.settings.mode = ControlMode::Manual;
                            self.settings.save().unwrap();
                            self.remote_receiver.as_mut().and_then(|receiver| Some(receiver.stop()));
                        }
                    });
                    ui.add_enabled_ui(self.settings.mode != ControlMode::Osc, |ui| {
                        if ui.button("Osc").clicked() {
                            self.settings.mode = ControlMode::Osc;
                            self.settings.save().unwrap();
                            self.remote_receiver.as_mut().and_then(|receiver| Some(receiver.stop()));
                        }
                    });
                    ui.add_enabled_ui(self.settings.mode == ControlMode::Manual || self.settings.mode == ControlMode::Osc, |ui| {
                        if ui.button("Remote").clicked() {
                            self.settings.mode = ControlMode::Remote(RemoteMode::Sender);
                            self.settings.save().unwrap();
                            self.remote_receiver.as_mut().and_then(|receiver| Some(receiver.stop()));
                        }
                    });
                });

                ui.add_space(10.0);

                // BLE Device selector
                ui.horizontal(|ui| {
                    let selected_device = self.found_devices
                        .iter()
                        .nth(self.selected_device as usize);

                    ui.label("Device:");
                    egui::ComboBox::from_id_salt("device_selector")
                        .selected_text(selected_device.and_then(|d| Some(d.get_name())).unwrap())
                        .show_ui(ui, |ui| {
                            for i in 0..self.found_devices.len() {
                                let device = self.found_devices.get(i).unwrap();
                                if ui.selectable_value(&mut self.selected_device, i as u16, device.get_name()).clicked() {
                                    self.connect_to_selected();
                                }
                            }
                        });
                });

                match (&self.adapter_status, self.selected_device) {
                    (_, 0) => {}
                    (Some(AdapterStatus::NotConnected), _) => { ui.colored_label(Color32::RED, "Not connected"); }
                    (Some(AdapterStatus::Connecting(_)), _) => { ui.colored_label(Color32::ORANGE, "Connecting..."); }
                    (Some(AdapterStatus::Connected(_)), _) => { ui.colored_label(Color32::GREEN, "Connected!"); }
                    _ => {}
                }

                ui.add_space(10.0);

                // Max intensity setting
                ui.horizontal(|ui| {
                    ui.label("Maximum Intensity:");
                    let response = ui.add(
                        egui::DragValue::new(&mut self.settings.max_intensity_percent)
                            .speed(0.1)
                            .range(0..=100),
                    );
                    if response.changed() {
                        self.settings.save().unwrap();
                    }
                    ui.label("%");
                });

                ui.add_space(10.0);

                // Advanced OSC settings
                if self.settings.mode == ControlMode::Osc {
                    ui.separator();
                    ui.add_space(10.0);

                    if ui.link(if self.show_advanced_settings { "Hide advanced OSC settings" } else { "Show advanced OSC settings" }).clicked() {
                        self.show_advanced_settings = !self.show_advanced_settings;
                    }

                    ui.add_space(10.0);

                    if self.show_advanced_settings {
                        // OSC Remap Range
                        ui.horizontal(|ui| {
                            ui.label("Range Start:");
                            let response = ui.add(
                                egui::DragValue::new(&mut self.settings.osc_range_start)
                                    .speed(0.1)
                                    .range(0.0..=1.0),
                            );
                            if response.changed() {
                                self.settings.save().unwrap();
                            }

                            ui.add_space(20.0);

                            ui.label("Range End:");
                            let response = ui.add(
                                egui::DragValue::new(&mut self.settings.osc_range_end)
                                    .speed(0.1)
                                    .range(0.0..=1.0),
                            );
                            if response.changed() {
                                self.settings.save().unwrap();
                            }
                        });

                        ui.add_space(10.0);

                        // OSC Address
                        ui.label("OSC Address:");
                        let response = ui.add(
                            egui::TextEdit::multiline(&mut self.settings.osc_path)
                                .desired_rows(2)
                                .desired_width(f32::INFINITY)
                        );
                        if response.changed() {
                            self.osc_server.set_pattern(WildMatch::new(self.settings.osc_path.as_str()));
                            self.settings.save().unwrap();
                        }

                        ui.add_space(10.0);

                        // OSC Debug
                        ui.horizontal(|ui| {
                            ui.label("Current OSC Value:");
                            ui.colored_label(Color32::CYAN, format!("{:.3}", self.osc_value.value));
                        });
                        if !self.osc_value.address.is_empty() {
                            ui.colored_label(Color32::GRAY, format!("{}", self.osc_value.address));
                        }
                        ui.add_space(10.0);
                    }
                }

                // Remote control settings
                let mut save_settings = false;
                if let ControlMode::Remote(mode) = &mut self.settings.mode {
                    ui.separator();
                    ui.add_space(10.0);

                    if ui.radio_value(mode, RemoteMode::Sender, "Sender").clicked() {
                        self.remote_receiver.as_mut().and_then(|receiver| Some(receiver.stop()));
                        save_settings = true;
                    }

                    ui.add_space(10.0);

                    ui.add_enabled_ui(mode == &RemoteMode::Sender, |ui| {
                        ui.label("Enter remote control code:");
                        ui.text_edit_singleline(&mut self.remote_sender.code);
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui.button("Connect").clicked() {
                                if let Ok(decoded) = base64::prelude::BASE64_STANDARD
                                    .decode(self.remote_sender.code.as_bytes())
                                    .map_err(anyhow::Error::from)
                                    .and_then(|decoded| String::from_utf8(decoded).map_err(anyhow::Error::from)) {
                                    let split = decoded.split("|").collect::<Vec<&str>>();
                                    if split.len() != 2 {
                                        return;
                                    }

                                    let url = split[0];
                                    let pairing_code = split[1];

                                    if let Ok(url) = Url::parse(url) {
                                        match self.remote_sender.connect_to(url, pairing_code) {
                                            Ok(_) => {
                                                self.sender_state = RemoteSenderState::Connected;
                                            }
                                            Err(error) => {
                                                self.sender_state = RemoteSenderState::Error(format!("{}", error));
                                            }
                                        }
                                    }
                                }
                            }
                            match &self.sender_state {
                                RemoteSenderState::NotConnected => ui.label("Not connected"),
                                RemoteSenderState::Connected => ui.colored_label(Color32::GREEN, "Connected"),
                                RemoteSenderState::Error(error) => ui.colored_label(Color32::RED, error),
                            };
                        });

                        ui.add_space(4.0);

                        if ui.checkbox(&mut self.settings.remote_sync_local, "Sync with local").clicked() {
                            save_settings = true;
                        }
                    });

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);

                    if ui.radio_value(mode, RemoteMode::Receiver, "Receiver").clicked() {
                        self.remote_receiver.as_mut().and_then(|receiver| Some(receiver.start()));
                        self.remote_sender.disconnect();
                        self.sender_state = RemoteSenderState::NotConnected;
                        save_settings = true;
                    }

                    ui.add_space(10.0);

                    let mut can_retry = false;
                    ui.add_enabled_ui(mode == &RemoteMode::Receiver && self.sender_url.is_some() && self.sender_pairing_code.is_some(), |ui| {
                        if ui.link("Copy remote control code").clicked() {
                            let url = self.sender_url.clone().unwrap();
                            let pairing_code = self.sender_pairing_code.clone().unwrap();
                            let code = url + "|" + pairing_code.as_str();
                            let code = base64::prelude::BASE64_STANDARD.encode(&code.as_bytes());
                            let mut clipbpard = arboard::Clipboard::new().unwrap();
                            clipbpard.set_text(code.as_str()).unwrap();
                            println!("{}", code);
                        }
                        ui.add_space(4.0);
                        match &self.receiver_state {
                            RemoteReceiverState::NotConnected => ui.label("Not connected"),
                            RemoteReceiverState::NoToken => ui.colored_label(Color32::ORANGE, "Missing ngrok token"),
                            RemoteReceiverState::Connecting => ui.colored_label(Color32::ORANGE, "Connecting..."),
                            RemoteReceiverState::Connected => ui.colored_label(Color32::DARK_GREEN, "Connected"),
                            RemoteReceiverState::Active => ui.colored_label(Color32::GREEN, "Active"),
                            RemoteReceiverState::Error(error) => {
                                let response = ui.colored_label(Color32::RED, error);
                                can_retry = true;
                                response
                            }
                        };
                    });

                    if can_retry && ui.button("Retry").clicked() {
                        self.remote_receiver.as_mut().and_then(|receiver| Some(receiver.start()));
                    }
                }

                if save_settings {
                    self.settings.save().unwrap();
                }
            });
        });

        if self.intensity != self.last_intensity || self.last_max_intensity_perc != self.settings.max_intensity_percent {
            self.last_intensity = self.intensity;
            self.last_max_intensity_perc = self.settings.max_intensity_percent;

            let speed_scale = self.settings.max_intensity_percent as f32 / 100.0;
            let intensity = (self.intensity as f32 * speed_scale) as u8;

            if let ControlMode::Remote(RemoteMode::Sender) = self.settings.mode {
                _ = self.remote_sender.send_speed(self.intensity as f32 / 20.0);
            }

            if self.settings.mode != ControlMode::Remote(RemoteMode::Sender) || self.settings.remote_sync_local {
                _ = match self.selected_device {
                    0 => self.generic_service.send_speed(intensity),
                    _ => self.gatt_service.send_speed(intensity),
                };
            }
        }

        ctx.request_repaint_after(Duration::from_millis(1000 / 30));
    }
}

enum DeviceProfile {
    GenericDevice,
    GattDevice(BluetoothGattDevice),
}

impl DeviceProfile {
    fn get_name(&self) -> String {
        match self {
            DeviceProfile::GenericDevice => "Generic Device".into(),
            DeviceProfile::GattDevice(device) => {
                device.device_name.clone().unwrap_or(device.device_address.clone())
            }
        }
    }
}

#[allow(unused)]
#[derive(Debug)]
enum AdapterStatus {
    NotConnected,
    Connecting(String),
    Connected(String),
}

enum RemoteSenderState {
    NotConnected,
    Connected,
    Error(String),
}

enum RemoteReceiverState {
    NoToken,
    NotConnected,
    Connecting,
    Connected,
    Active,
    Error(String),
}