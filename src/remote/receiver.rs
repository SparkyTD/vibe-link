use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;
use tokio::sync::mpsc::{channel as tokio_channel, Receiver as TokioReceiver, Sender as TokioSender};
use tokio::net::TcpListener;
use tokio::io::AsyncReadExt;
use tokio::select;
use ngrok::Session;
use ngrok::config::ForwarderBuilder;
use ngrok::forwarder::Forwarder;
use ngrok::tunnel::TcpTunnel;
use ngrok::tunnel::EndpointInfo;
use ngrok::tunnel::TunnelInfo;
use url::Url;
use uuid::Uuid;

pub struct RemoteControlServer {
    server_rx: Receiver<ServerMessage>,
    server_tx: TokioSender<ServerCommand>,
}

impl RemoteControlServer {
    pub fn new(ngrok_token: &str) -> Self {
        let (gui_tx, server_rx) = channel::<ServerMessage>();
        let (server_tx, gui_rx) = tokio_channel::<ServerCommand>(100);

        let token = ngrok_token.into();
        tokio::spawn(async move {
            Self::server_task(gui_tx, gui_rx, token).await;
        });

        Self {
            server_rx,
            server_tx,
        }
    }

    pub fn start(&self) -> anyhow::Result<()> {
        let server_tx = self.server_tx.clone();
        tokio::spawn(async move {
            server_tx.send(ServerCommand::Start).await.unwrap();
        });
        Ok(())
    }

    pub fn stop(&self) -> anyhow::Result<()> {
        let server_tx = self.server_tx.clone();
        tokio::spawn(async move {
            server_tx.send(ServerCommand::Stop).await.unwrap();
        });
        Ok(())
    }

    pub fn recv_message(&mut self) -> Option<ServerMessage> {
        self.server_rx.try_recv().ok()
    }

    async fn server_task(gui_tx: Sender<ServerMessage>, gui_rx: TokioReceiver<ServerCommand>, ngrok_token: String) {
        let mut state = ServerLoopState {
            active_tunnel: None,
            active_session: None,
            listener: None,
            auth_token: Uuid::new_v4().to_string(),
            ngrok_token,
            gui_tx,
            gui_rx,
        };

        loop {
            match Self::server_loop(&mut state).await {
                Ok(true) => continue,
                Ok(false) => break,
                Err(error) => {
                    _ = state.gui_tx.send(ServerMessage::Error {
                        message: error.to_string(),
                    });
                    continue;
                }
            }
        }
    }

    async fn server_loop(state: &mut ServerLoopState) -> anyhow::Result<bool> {
        let ngrok_token = state.ngrok_token.clone();
        select! {
                // Handle commands from the channel
                Some(command) = state.gui_rx.recv() => {
                    match command {
                        ServerCommand::Start => {
                            if state.active_tunnel.is_none() {
                                let _ = state.gui_tx.send(ServerMessage::Initializing);
                                let new_listener = TcpListener::bind("0.0.0.0:0").await?;
                                let local_addr = new_listener.local_addr()?;

                                let session = Session::builder()
                                    .heartbeat_interval(Duration::from_secs(10))?
                                    .heartbeat_tolerance(Duration::from_secs(1))?
                                    .authtoken(ngrok_token)
                                    .connect()
                                    .await?;

                                let tunnel = session
                                    .tcp_endpoint()
                                    .listen_and_forward(Url::parse(
                                        &format!("tcp://{}", local_addr)
                                    )?)
                                    .await?;

                                println!("Tunnel: {}", tunnel.url());
                                let _ = state.gui_tx.send(ServerMessage::Started {
                                    url: tunnel.url().to_string(),
                                    token: state.auth_token.clone(),
                                });

                                state.active_session = Some(session);
                                state.active_tunnel = Some(tunnel);
                                state.listener = Some(new_listener);
                            }
                        }
                        ServerCommand::Stop => {
                            if let (Some(tunnel), Some(mut session)) = (state.active_tunnel.take(), state.active_session.take()) {
                                _ = session.close_tunnel(tunnel.id());
                                let result = session.close().await;
                                drop(tunnel);
                                drop(session);

                                println!("Session closed: {:?}", result);
                            }

                            _ = state.listener.take();
                            let _ = state.gui_tx.send(ServerMessage::Stopped);
                        }
                    }
                }

                // Handle incoming connections (only when listener exists)
                Some(accept_result) = async {
                    match &state.listener {
                        Some(l) => Some(l.accept().await),
                        None => None
                    }
                } => {
                    if let Ok((mut stream, addr)) = accept_result {
                        println!("Connection from: {}", addr);
                        let gui_tx = state.gui_tx.clone();
                        _ = gui_tx.send(ServerMessage::NewConnection);

                        // Spawn a task to handle this connection
                        let auth_token = state.auth_token.clone();
                        let tunnel = state.active_tunnel.as_ref().unwrap();
                        let tunnel_url = tunnel.url();
                        let tunnel_url = tunnel_url.to_string();
                        tokio::spawn(async move {
                            let mut buffer = [0u8; 1024];
                            let mut is_authenticated = false;
                            loop {
                                match stream.read(&mut buffer).await {
                                    Ok(0) => break, // Connection closed
                                    Ok(length) => {
                                        if !is_authenticated && length == 36 {
                                            if let Ok(token) = String::from_utf8(buffer[..length].to_vec()) {
                                                if token == auth_token {
                                                    is_authenticated = true;
                                                    continue;
                                                }
                                            }
                                        }

                                        if !is_authenticated {
                                            println!("Unauthenticated message received, closing connection.");
                                            _ = gui_tx.send(ServerMessage::Stopped);
                                            break;
                                        }

                                        buffer[..length].windows(4).for_each(|chunk| {
                                            let _ = gui_tx.send(ServerMessage::SpeedReceived {
                                                speed: f32::from_le_bytes(chunk.try_into().unwrap()),
                                            });
                                        });
                                    }
                                    Err(e) => {
                                        eprintln!("Read error: {}", e);
                                        break;
                                    }
                                }
                            }
                            _ = gui_tx.send(ServerMessage::Started {
                                url: tunnel_url,
                                token: ngrok_token.clone()
                            });
                        });
                    }
                }

                else => return Ok(false) // All channels closed
            }

        Ok(true)
    }
}

struct ServerLoopState {
    active_tunnel: Option<Forwarder<TcpTunnel>>,
    active_session: Option<Session>,
    listener: Option<TcpListener>,
    auth_token: String,
    ngrok_token: String,
    gui_tx: Sender<ServerMessage>,
    gui_rx: TokioReceiver<ServerCommand>,
}

#[derive(Debug)]
pub enum ServerCommand {
    Start,
    Stop,
}

#[derive(Debug)]
pub enum ServerMessage {
    Initializing,
    Started { url: String, token: String },
    Stopped,
    NewConnection,
    SpeedReceived { speed: f32 },
    Error { message: String },
}