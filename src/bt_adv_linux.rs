#[cfg(target_os = "linux")]
pub mod ble_adv {
    use std::collections::BTreeMap;
    use std::time::Duration;
    use bluer::adv::{Advertisement, AdvertisementHandle, Type};
    use bluer::{Adapter, Session};
    use crate::bt_generic::BleAdvertiserTrait;

    pub struct BleAdvertiserLinux {
        session: Option<Session>,
        adapter: Option<Adapter>,
        adv_handle: Option<AdvertisementHandle>,
    }

    impl BleAdvertiserLinux {
        pub fn new() -> Self {
            Self {
                session: None,
                adapter: None,
                adv_handle: None,
            }
        }
    }

    impl BleAdvertiserTrait for BleAdvertiserLinux {
        async fn init(&mut self) -> anyhow::Result<()> {
            drop(self.session.take());

            let session = Session::new().await?;
            let adapter = match  session.default_adapter().await {
                Ok(adapter) => adapter,
                Err(error) => {
                    return Err(anyhow::anyhow!("Error getting default adapter: {}", error));
                },
            };

            self.session.replace(session);
            self.adapter.replace(adapter);

            Ok(())
        }

        async fn send(&mut self, mfr_id: u16, data: &[u8]) -> anyhow::Result<()> {
            let mut manufacturer_data = BTreeMap::new();
            manufacturer_data.insert(mfr_id, data.to_vec());

            let advertisement = Advertisement {
                advertisement_type: Type::Peripheral,
                manufacturer_data,
                duration: Some(Duration::from_millis(100)),
                timeout: Some(Duration::from_secs(0)),
                min_interval: Some(Duration::from_millis(50)),
                max_interval: Some(Duration::from_millis(50)),
                tx_power: Some(20),
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