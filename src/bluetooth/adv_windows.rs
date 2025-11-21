// https://github.com/buttplugio/docs.buttplug.io/issues/2

#[cfg(target_os = "windows")]
pub mod ble_adv {
    use windows::{
        Devices::Bluetooth::Advertisement::{
            BluetoothLEAdvertisement, BluetoothLEAdvertisementPublisher,
            BluetoothLEManufacturerData,
        },
        Storage::Streams::DataWriter,
    };
    use windows::Foundation::TypedEventHandler;
    use crate::bluetooth::generic::BleAdvertiser;

    pub struct BleAdvertiserWindows {
        publisher: Option<BluetoothLEAdvertisementPublisher>,
    }

    impl BleAdvertiserWindows {
        pub fn new() -> Self {
            Self {
                publisher: None,
            }
        }
    }

    impl BleAdvertiser for BleAdvertiserWindows {
        async fn init(&mut self) -> anyhow::Result<()> {
            return Ok(());
            // Stop and drop any existing publisher
            if let Some(publisher) = self.publisher.take() {
                let _ = publisher.Stop();
            }

            // Create new publisher
            let publisher = BluetoothLEAdvertisementPublisher::new()
                .map_err(|e| anyhow::anyhow!("Failed to create BLE publisher: {}", e))?;

            self.publisher = Some(publisher);
            Ok(())
        }

        async fn send(&mut self, _mfr_id: u16, _data: &[u8]) -> anyhow::Result<()> {
            return Ok(());
            let publisher = self.publisher.as_ref()
                .ok_or_else(|| anyhow::anyhow!("Publisher not initialized. Call init() first."))?;

            // Stop current advertisement if running
            let _ = publisher.Stop();

            // Create new advertisement
            let advertisement = publisher.Advertisement()?;

            // Clear any existing manufacturer data
            advertisement.ManufacturerData()
                .map_err(|e| anyhow::anyhow!("Failed to access manufacturer data: {}", e))?
                .Clear()
                .map_err(|e| anyhow::anyhow!("Failed to clear manufacturer data: {}", e))?;

            // Create manufacturer data with company ID and payload
            let mfr_data = BluetoothLEManufacturerData::new()
                .map_err(|e| anyhow::anyhow!("Failed to create manufacturer data: {}", e))?;

            mfr_data.SetCompanyId(_mfr_id)
                .map_err(|e| anyhow::anyhow!("Failed to set company ID: {}", e))?;

            // Write payload using DataWriter
            let writer = DataWriter::new()
                .map_err(|e| anyhow::anyhow!("Failed to create data writer: {}", e))?;
            writer.WriteBytes(_data)
                .map_err(|e| anyhow::anyhow!("Failed to write payload data: {}", e))?;

            let buffer = writer.DetachBuffer()
                .map_err(|e| anyhow::anyhow!("Failed to detach buffer: {}", e))?;
            mfr_data.SetData(&buffer)
                .map_err(|e| anyhow::anyhow!("Failed to set manufacturer data payload: {}", e))?;

            // Add manufacturer data to advertisement
            advertisement.ManufacturerData()
                .map_err(|e| anyhow::anyhow!("Failed to get manufacturer data collection: {}", e))?
                .Append(&mfr_data)
                .map_err(|e| anyhow::anyhow!("Failed to append manufacturer data: {}", e))?;

            publisher.Start()
                .map_err(|e| anyhow::anyhow!("Failed to start advertising: {}", e))?;

            Ok(())
        }
    }
}