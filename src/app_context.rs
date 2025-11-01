use std::time::Instant;
use crate::bt_gatt::{BleMessage, BluetoothGattDevice, BluetoothGattService};
use egui::{CentralPanel, Color32, Context, SidePanel, TopBottomPanel};
use eframe::Frame;
use wildmatch::WildMatch;
use crate::bt_generic::BluetoothGenericService;
use crate::speed_filter::SpeedFilter;
use crate::osc_server::{OscFloatData, OscServer};
use crate::settings::Settings;

pub struct AppContext {
    intensity: u8,
    last_intensity: u8,
    settings: Settings,
    osc_server: OscServer,
    osc_value: OscFloatData,
    selected_device: u16,
    gatt_service: BluetoothGattService,
    generic_service: BluetoothGenericService,
    adapter_initialized: bool,
    adapter_error: Option<String>,
    adapter_status: Option<AdapterStatus>,
    found_devices: Vec<DeviceProfile>,
    filter: SpeedFilter,
    last_filter_update: Instant,
}

impl AppContext {
    pub fn new() -> Self {
        let settings = Settings::load_or_default().unwrap();
        let mut osc_server = OscServer::new(9001);
        osc_server.set_pattern(WildMatch::new(&settings.osc_path));
        Self {
            intensity: 0,
            last_intensity: 0,
            settings,
            osc_server,
            osc_value: OscFloatData::default(),
            selected_device: 0,
            gatt_service: BluetoothGattService::new(),
            generic_service: BluetoothGenericService::new(),
            adapter_initialized: false,
            adapter_error: None,
            adapter_status: None,
            found_devices: vec![DeviceProfile::GenericDevice],
            filter: SpeedFilter::new(0.05),
            last_filter_update: Instant::now(),
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
}

impl eframe::App for AppContext {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        if let Some(val) = self.osc_server.try_read_value() {
            self.osc_value = val;
        }

        if self.settings.osc_mode {
            let delta_time = self.last_filter_update.elapsed().as_secs_f32();
            self.last_filter_update = Instant::now();

            let scaled_value = ((self.osc_value.value - self.settings.osc_range_start) / (self.settings.osc_range_end - self.settings.osc_range_start)).clamp(0.0, 1.0);
            let speed_value = self.filter.update(scaled_value, delta_time).clamp(0.0, 5.0);

            self.send_speed(speed_value / 5.0);
        }

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

        // Draw top bar
        TopBottomPanel::top("title_bar").show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.heading("VibeLink");

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
        if !self.settings.osc_mode {
            SidePanel::right("side_panel")
                .resizable(false)
                .default_width(0.0)
                .show(ctx, |ui| {
                    let available_height = ui.available_height();
                    ui.horizontal(|ui| {
                        ui.add_space(10.0);
                        ui.vertical(|ui| {
                            ui.add_space(20.0);
                            ui.spacing_mut().slider_width = available_height - 40.0;
                            let slider_max = match self.selected_device {
                                0 => 7,
                                _ => 20,
                            };
                            ui.add(
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
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label("OSC Mode:");
                    if ui.checkbox(&mut self.settings.osc_mode, "").changed() {
                        self.settings.save().unwrap();
                    }
                });

                ui.add_space(10.0);

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

                if self.settings.osc_mode {
                    ui.horizontal(|ui| {
                        ui.label("Range start:");
                        let response = ui.add(
                            egui::DragValue::new(&mut self.settings.osc_range_start)
                                .speed(0.1)
                                .range(0.0..=1.0),
                        );
                        if response.changed() {
                            self.settings.save().unwrap();
                        }

                        ui.add_space(20.0);

                        ui.label("Range end:");
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

                    ui.label("OSC Path:");
                    let response = ui.add(
                        egui::TextEdit::multiline(&mut self.settings.osc_path)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY)
                    );
                    if response.changed() {
                        self.osc_server.set_pattern(WildMatch::new(self.settings.osc_path.as_str()));
                        self.settings.save().unwrap();
                    }

                    ui.add_space(10.0);

                    ui.label("Current OSC value:");
                    ui.colored_label(Color32::CYAN, format!("{:.4}", self.osc_value.value));
                    ui.colored_label(Color32::GRAY, format!("{}", self.osc_value.address));
                }
            });
        });

        if self.intensity != self.last_intensity {
            self.last_intensity = self.intensity;

            _ = match self.selected_device {
                0 => self.generic_service.send_speed(self.intensity),
                _ => self.gatt_service.send_speed(self.intensity),
            };
        }

        ctx.request_repaint();
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