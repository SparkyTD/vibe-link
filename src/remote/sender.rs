use std::io::Write;
use std::net::{TcpStream};
use url::Url;

pub struct RemoteControlSender {
    pub code: String,
    stream: Option<TcpStream>,
}

impl RemoteControlSender {
    pub fn new() -> Self {
        Self {
            code: String::new(),
            stream: None,
        }
    }

    pub fn connect_to(&mut self, url: Url, pairing_code: &str) -> anyhow::Result<()> {
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }

        let address = format!("{}:{}", url.host_str().unwrap(), url.port_or_known_default().unwrap());
        let mut stream = TcpStream::connect(address)?;
        stream.write_all(pairing_code.as_bytes())?;

        self.stream.replace(stream);

        Ok(())
    }
    
    pub fn send_speed(&mut self, speed: f32) -> anyhow::Result<()> {
        if let Some(stream) = self.stream.as_mut() {
            stream.write_all(&speed.to_le_bytes())?;
        }
        
        Ok(())
    }
}