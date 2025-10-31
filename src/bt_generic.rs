#[cfg(target_os = "linux")]
pub mod ble_linux {
    use std::collections::BTreeMap;
    use std::time::Duration;
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::thread;
    use bluer::adv::{Advertisement, AdvertisementHandle, Type};
    use bluer::Session;

    const COMPANY_ID: u16 = 0xFFF0;
    const RAW_ADDRESS: [u8; 5] = [0x77, 0x62, 0x4d, 0x53, 0x45];

    pub struct BluetoothGenericService {
        pub gui_tx: Sender<u8>,
    }

    impl BluetoothGenericService {
        pub fn new() -> Self {
            let (gui_tx, ble_rx) = channel::<u8>();

            thread::spawn(move || {
                Self::ble_thread(ble_rx);
            });

            Self {
                gui_tx
            }
        }

        fn ble_thread(ble_rx: Receiver<u8>) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let session = Session::new().await.unwrap();
                let adapter = session.default_adapter().await.unwrap();
                let mut adv_handle: Option<AdvertisementHandle> = None;

                loop {
                    if let Ok(speed) = ble_rx.recv() {
                        let command = match speed {
                            1 => Command::Raw([0xF4, 0x00, 0x00]),
                            2 => Command::Raw([0xF7, 0x00, 0x00]),
                            3 => Command::Raw([0xF6, 0x00, 0x00]),
                            4 => Command::Raw([0xF1, 0x00, 0x00]),
                            5 => Command::Raw([0xF3, 0x00, 0x00]),
                            6 => Command::Raw([0xE7, 0x00, 0x00]),
                            7 => Command::Raw([0xE6, 0x00, 0x00]),
                            _ => Command::Raw([0xE5, 0x00, 0x00]),
                        };

                        let command = BleUtil::get_ble_command(&RAW_ADDRESS, command);
                        let mut final_command = vec![0x02, 0x01, 0x06];
                        final_command.extend(command);

                        let mut manufacturer_data = BTreeMap::new();
                        manufacturer_data.insert(COMPANY_ID, final_command.clone());

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

                        if let Some(handle) = adv_handle.take() {
                            drop(handle);
                        }

                        adv_handle.replace(adapter.advertise(advertisement).await.unwrap());
                        println!("Speed set to {}", speed);
                    }
                }
            });
        }

        pub fn send_speed(&self, speed: u8) -> anyhow::Result<()> {
            self.gui_tx.send(speed)?;
            Ok(())
        }
    }

    #[allow(unused)]
    pub enum Command {
        Raw([u8; 3]),
        Byte(u8),
    }

    pub struct BleUtil;

    impl BleUtil {
        pub fn get_ble_command(address_bytes: &[u8; 5], command_bytes: Command) -> Vec<u8> {
            let addr_len = address_bytes.len();
            let total_len = addr_len + 1 + 5;
            let mut result = vec![0u8; total_len];

            match command_bytes {
                Command::Byte(val) => {
                    Self::get_rf_payload(&address_bytes, &vec![val], &mut result);

                    result
                }
                Command::Raw(bytes) => {
                    Self::get_rf_payload(&address_bytes, &vec![0], &mut result);
                    result[8..11].copy_from_slice(&bytes);

                    result
                }
            }
        }

        fn get_rf_payload(addr: &[u8; 5], data: &[u8], result: &mut [u8]) {
            let mut ctx_25 = [0u8; 7];
            let mut ctx_3f = [0u8; 7];

            Self::whitening_init(0x25, &mut ctx_25);
            Self::whitening_init(0x3f, &mut ctx_3f);

            let length_24 = 0x12 + addr.len() + data.len();
            let length_26 = length_24 + 0x02;

            let mut result_25 = vec![0u8; length_26];
            let mut result_3f = vec![0u8; length_26];
            let mut result_buf = vec![0u8; length_26];

            // Set constant values
            result_buf[0x0f] = 0x71;
            result_buf[0x10] = 0x0f;
            result_buf[0x11] = 0x55;

            // Flip and write address
            for j in 0..addr.len() {
                result_buf[0x12 + addr.len() - j - 1] = addr[j];
            }

            // Flip and write data
            for j in 0..data.len() {
                result_buf[length_24 - j - 1] = data[j];
            }

            // Invert bytes
            for i in 0..(3 + addr.len()) {
                result_buf[0x0f + i] = Self::invert_8(result_buf[0x0f + i]);
            }

            // Calculate and write CRC16
            let crc16 = Self::check_crc16(addr, data);
            result_buf[length_24] = (crc16 & 0xff) as u8;
            result_buf[length_24 + 1] = ((crc16 >> 8) & 0xff) as u8;

            // Whitening encode
            Self::whitening_encode(
                &result_buf,
                2 + addr.len() + data.len(),
                &mut ctx_3f,
                0x12,
                &mut result_3f,
            );
            Self::whitening_encode(&result_buf, length_26, &mut ctx_25, 0x00, &mut result_25);

            // XOR results
            for i in 0..length_26 {
                result_25[i] ^= result_3f[i];
            }

            // Copy final result
            result[..11].copy_from_slice(&result_25[0x0f..0x1a]);
        }

        fn whitening_init(val: u8, ctx: &mut [u8; 7]) {
            ctx[0] = 1;
            ctx[1] = (val >> 5) & 1;
            ctx[2] = (val >> 4) & 1;
            ctx[3] = (val >> 3) & 1;
            ctx[4] = (val >> 2) & 1;
            ctx[5] = (val >> 1) & 1;
            ctx[6] = val & 1;
        }

        fn check_crc16(addr: &[u8], data: &[u8]) -> u16 {
            let mut crc: u32 = 0xffff;

            // Process address bytes (reversed)
            for i in (0..addr.len()).rev() {
                crc ^= (addr[i] as u32) << 8;
                for _ in 0..8 {
                    if (crc & 0x8000) != 0 {
                        crc = (crc << 1) ^ 0x1021;
                    } else {
                        crc <<= 1;
                    }
                }
            }

            // Process data bytes
            for i in 0..data.len() {
                crc ^= (Self::invert_8(data[i]) as u32) << 8;
                for _ in 0..8 {
                    if (crc & 0x8000) != 0 {
                        crc = (crc << 1) ^ 0x1021;
                    } else {
                        crc <<= 1;
                    }
                }
            }

            crc = (!Self::invert_16(crc as u16)) as u32 & 0xffff;
            crc as u16
        }

        pub fn invert_8(mut value: u8) -> u8 {
            let mut result: u8 = 0;
            for _ in 0..8 {
                result <<= 1;
                result |= value & 1;
                value >>= 1;
            }
            result
        }

        fn invert_16(value: u16) -> u16 {
            let mut result = 0u16;
            let mut val = value;
            for _ in 0..16 {
                result <<= 1;
                result |= val & 1;
                val >>= 1;
            }
            result
        }

        pub fn whitening_encode(
            data: &[u8],
            len: usize,
            ctx: &mut [u8],
            offset: usize,
            result: &mut [u8],
        ) {
            // Copy data to result
            result[..len].copy_from_slice(&data[..len]);

            for i in 0..len {
                let var6 = ctx[6] as i8 as i32;
                let var5 = ctx[5] as i8 as i32;
                let var4 = ctx[4] as i8 as i32;
                let var3 = ctx[3] as i8 as i32;
                let var52 = var5 ^ ctx[2] as i8 as i32;
                let var41 = var4 ^ ctx[1] as i8 as i32;
                let var63 = var6 ^ ctx[3] as i8 as i32;
                let var630 = var63 ^ ctx[0] as i8 as i32;

                ctx[0] = (var52 ^ var6) as u8;
                ctx[1] = var630 as u8;
                ctx[2] = var41 as u8;
                ctx[3] = var52 as u8;
                ctx[4] = (var52 ^ var3) as u8;
                ctx[5] = (var630 ^ var4) as u8;
                ctx[6] = (var41 ^ var5) as u8;

                let c = result[i + offset] as i8 as i32;
                result[i + offset] = (((c & 0x80) ^ ((var52 ^ var6) << 7))
                    + ((c & 0x40) ^ (var630 << 6))
                    + ((c & 0x20) ^ (var41 << 5))
                    + ((c & 0x10) ^ (var52 << 4))
                    + ((c & 0x08) ^ (var63 << 3))
                    + ((c & 0x04) ^ (var4 << 2))
                    + ((c & 0x02) ^ (var5 << 1))
                    + ((c & 0x01) ^ var6)) as u8;
            }
        }
    }
}