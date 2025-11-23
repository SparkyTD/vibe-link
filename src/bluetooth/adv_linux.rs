#[cfg(target_os = "linux")]
pub mod ble_adv {
    use std::collections::{BTreeMap, HashMap};
    use std::io::Write;
    use std::time::Duration;
    use bluer::adv::{Advertisement, AdvertisementHandle, Type};
    use bluer::{Adapter, Session};
    use serialport::SerialPort;
    use crate::bluetooth::generic::BleAdvertiser;

    pub struct BleAdvertiserLinux {
        session: Option<Session>,
        adapter: Option<Adapter>,
        adv_handle: Option<AdvertisementHandle>,
        max_tx_power: i16,

        serial_port: Option<Box<dyn SerialPort>>,
        speed_dict: HashMap<u8, u8>,
    }

    impl BleAdvertiserLinux {
        pub fn new() -> Self {
            Self {
                session: None,
                adapter: None,
                adv_handle: None,
                max_tx_power: 20,

                serial_port: None,
                speed_dict: HashMap::new(),
            }
        }
    }

    impl BleAdvertiser for BleAdvertiserLinux {
        async fn init(&mut self) -> anyhow::Result<()> {
            self.speed_dict.insert(0xE5, b'0');
            self.speed_dict.insert(0xF4, b'1');
            self.speed_dict.insert(0xF7, b'2');
            self.speed_dict.insert(0xF6, b'3');
            self.speed_dict.insert(0xF1, b'4');
            self.speed_dict.insert(0xF3, b'5');
            self.speed_dict.insert(0xE7, b'6');
            self.speed_dict.insert(0xE6, b'7');

            // return Ok(());

            drop(self.session.take());

            let session = Session::new().await?;
            let adapter = match  session.default_adapter().await {
                Ok(adapter) => adapter,
                Err(error) => {
                    return Err(anyhow::anyhow!("Error getting default adapter: {}", error));
                },
            };

            let capabilities = adapter.supported_advertising_capabilities().await?;
            if let Some(capabilities) = capabilities {
                self.max_tx_power = capabilities.max_tx_power;
            }

            self.session.replace(session);
            self.adapter.replace(adapter);

            Ok(())
        }

        async fn send(&mut self, mfr_id: u16, data: &[u8]) -> anyhow::Result<()> {
            if let Some(port) = &mut self.serial_port {
                let speed = self.speed_dict[&data[11]];
                port.write_all(&[speed])?;
                // 0201066db643ce97fe427ce60000
            }

            let mut manufacturer_data = BTreeMap::new();
            manufacturer_data.insert(mfr_id, data.to_vec());

            let advertisement = Advertisement {
                advertisement_type: Type::Peripheral,
                manufacturer_data,
                duration: None,
                timeout: None,
                min_interval: Some(Duration::from_millis(20)),
                max_interval: Some(Duration::from_millis(20)),
                tx_power: Some(self.max_tx_power),
                ..Default::default()
            };

            if let Some(handle) = self.adv_handle.take() {
                drop(handle);
            }

            if let Some(adapter) = &self.adapter {
                self.adv_handle.replace(adapter.advertise(advertisement).await?);
            }

            Ok(())
        }
    }
}