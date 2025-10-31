use crate::bt_gatt::{BleMessage, BluetoothGattDevice, BluetoothGattService};
use eframe::Frame;
use egui::{CentralPanel, Context, SidePanel, TopBottomPanel};

#[cfg(target_os = "linux")]
use crate::bt_generic::ble_linux::BluetoothGenericService;

pub struct AppContext {
    osc_mode: bool,
    intensity: u8,
    last_intensity: u8,
    osc_range_start: f32,
    osc_range_end: f32,
    selected_device: u16,
    gatt_service: BluetoothGattService,
    #[cfg(target_os = "linux")]
    generic_service: BluetoothGenericService,
    adapter_initialized: bool,
    found_devices: Vec<DeviceProfile>,
}

impl AppContext {
    pub fn new() -> Self {
        Self {
            osc_mode: false,
            intensity: 0,
            last_intensity: 0,
            osc_range_start: 0.0,
            osc_range_end: 1.0,
            selected_device: 0,
            gatt_service: BluetoothGattService::new(),
            #[cfg(target_os = "linux")]
            generic_service: BluetoothGenericService::new(),
            adapter_initialized: false,
            found_devices: vec![DeviceProfile::GenericDevice],
        }
    }
}

impl eframe::App for AppContext {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        while let Some(message) = self.gatt_service.fetch_ble_message() {
            match message {
                BleMessage::AdapterInitialized => self.adapter_initialized = true,
                BleMessage::AdapterError(error) => {
                    self.adapter_initialized = false;
                    println!("Adapter Error: {}", error);
                }
                BleMessage::DeviceDiscovered(device) => {
                    self.found_devices.push(DeviceProfile::GattDevice(device));
                }
                BleMessage::DeviceConnected(_) => {}
                BleMessage::DeviceDisconnected(_) => {}
            }
        }

        // Draw top bar
        TopBottomPanel::top("title_bar").show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.heading("VibeLink");

                // Show error and retry button if adapter failed to initialize
                // (e.g. BT disabled on OS level, or no adapter connected)
                /*if self.active_adapter.is_none() {
                    ui.horizontal(|ui| {
                        ui.colored_label(Color32::RED, "No bluetooth adapter found");
                        if ui.button("Try again").clicked() {
                            self.try_init_ble();
                        }
                    });
                    ui.add_space(2.0);
                }*/
            });
        });

        // Draw intensity slider
        if !self.osc_mode {
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
                    ui.checkbox(&mut self.osc_mode, "");
                });

                ui.add_space(10.0);

                if self.osc_mode {
                    ui.horizontal(|ui| {
                        ui.label("Range start:");
                        ui.add(
                            egui::DragValue::new(&mut self.osc_range_start)
                                .speed(0.1)
                                .range(0.0..=1.0),
                        );

                        ui.add_space(20.0);

                        ui.label("Range end:");
                        ui.add(
                            egui::DragValue::new(&mut self.osc_range_end)
                                .speed(0.1)
                                .range(0.0..=1.0),
                        );
                    });

                    ui.add_space(10.0);
                }

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
                                    self.gatt_service.disconnect().unwrap();
                                    match device {
                                        DeviceProfile::GenericDevice => {
                                            self.gatt_service.disconnect().unwrap();
                                        }
                                        DeviceProfile::GattDevice(device) => {
                                            self.gatt_service.connect(device).unwrap();
                                            // self.gatt_service.send_data(b"Vibrate:20;").unwrap();
                                        }
                                    }
                                }
                            }
                        });
                });
            });
        });

        if self.intensity != self.last_intensity {
            self.last_intensity = self.intensity;

            _ = match self.selected_device {
                #[cfg(target_os = "linux")]
                0 => self.generic_service.send_speed(self.intensity),
                _ => self.gatt_service.send_data(format!("Vibrate:{};", self.intensity).as_bytes()),
            };

        }
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