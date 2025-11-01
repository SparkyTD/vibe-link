// https://github.com/buttplugio/docs.buttplug.io/issues/2

#[cfg(target_os = "windows")]
pub mod ble_adv {
    use windows::Devices::Bluetooth::Advertisement::{BluetoothLEAdvertisement, BluetoothLEAdvertisementPublisher, BluetoothLEAdvertisementPublisherStatus, BluetoothLEAdvertisementPublisherStatusChangedEventArgs, BluetoothLEManufacturerData};
    use windows::Foundation::TypedEventHandler;
    use windows::Storage::Streams::DataWriter;
    use crate::bt_generic::BleAdvertiserTrait;

    pub struct BleAdvertiserWindows {
        publisher: Option<BluetoothLEAdvertisementPublisher>
    }

    impl BleAdvertiserWindows {
        pub fn new() -> Self {
            Self {
                publisher: None,
            }
        }
    }

    impl BleAdvertiserTrait for BleAdvertiserWindows {
        async fn init(&mut self) -> anyhow::Result<()> {
            drop(self.publisher.take());

            let publisher = BluetoothLEAdvertisementPublisher::new()?;
            publisher.SetPreferredTransmitPowerLevelInDBm(None)?;

            self.publisher.replace(publisher);

            Ok(())
        }

        async fn send(&mut self, mfr_id: u16, data: &[u8]) -> anyhow::Result<()> {
            return Ok(());
            if let Some(publisher) = &self.publisher {
                println!("Sending payload: {}", hex::encode(data));

                let advertisement = publisher.Advertisement()?;
                let manufacturer_data = Self::create_manufacturer_date(mfr_id, data)?;
                let data_sections = advertisement.ManufacturerData()?;
                data_sections.Clear()?;
                data_sections.Append(&manufacturer_data)?;
                publisher.SetUseExtendedAdvertisement(false)?;
                publisher.SetIsAnonymous(false)?;
                publisher.Stop()?;
                publisher.Start()?;
            }

            Ok(())
        }
    }

    impl BleAdvertiserWindows {
        fn create_manufacturer_date(mfr_id: u16, data: &[u8]) -> anyhow::Result<BluetoothLEManufacturerData> {
            let data_writer = DataWriter::new()?;
            data_writer.WriteBytes(data)?;
            let buffer = data_writer.DetachBuffer()?;
            let manufacturer_data = BluetoothLEManufacturerData::Create(mfr_id, &buffer)?;
            Ok(manufacturer_data)
        }
    }
}