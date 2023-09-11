use std::{net::SocketAddr, sync::Arc, time::Duration};

use byteorder::{ByteOrder, LittleEndian};
use tokio::{self, net::TcpStream, runtime::Runtime, sync::broadcast, task::JoinHandle};
use tokio_tungstenite::tungstenite::{error::Error as TError, Message};

use futures::{SinkExt, StreamExt};
pub type FrameData = Vec<f32>;
pub type FrameSender = broadcast::Sender<FrameData>;
pub type FrameReceiver = broadcast::Receiver<FrameData>;

async fn handle_connection(
    peer: SocketAddr,
    stream: TcpStream,
    mut receiver: FrameReceiver,
) -> Result<(), TError> {
    let mut ws_stream = tokio_tungstenite::accept_async(stream).await?;
    log::info!("[{peer}] Established websocket connection");

    loop {
        tokio::select! {
            msg = ws_stream.next() => {
                match msg {
                    Some(msg) => {
                        let msg = msg?;
                        if msg.is_text() ||msg.is_binary() {
                            log::info!("[{peer}]: {msg}");
                        } else if msg.is_close() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            msg = receiver.recv() => {
                match msg {
                    Ok(frame_data) => {
                        let mut binary_data = vec![0u8; frame_data.len() * 4];
                        LittleEndian::write_f32_into(&frame_data, &mut binary_data);
                        ws_stream.send(Message::binary(binary_data)).await?;
                    }
                    Err(err) => {
                        log::error!("Failed to read next data frame: {}", err);
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn accept_connection(stream: TcpStream, receiver: FrameReceiver) {
    let peer = stream
        .peer_addr()
        .expect("connected streams should have a peer address");
    log::info!("[{peer}] New connection");
    match handle_connection(peer, stream, receiver).await {
        Ok(()) | Err(TError::ConnectionClosed | TError::Protocol(_) | TError::Utf8) => (),
        Err(err) => log::error!("[{peer}] Error processing connection: {}", err),
    }
}

async fn run_server(sender: Arc<FrameSender>) {
    let addr = "127.0.0.1:9090";
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Can't listen");
    log::info!("Listening on: {}", addr);

    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(accept_connection(stream, sender.subscribe()));
    }
}

pub struct Server {
    receiver: FrameReceiver,
    runtime: Runtime,
    thread_handle: JoinHandle<()>,
}

impl Server {
    pub fn start() -> (Server, Arc<FrameSender>) {
        let (sender, receiver) = tokio::sync::broadcast::channel(10);
        let sender = Arc::new(sender);

        // Start server.
        let runtime = Runtime::new().unwrap();
        let thread_handle = runtime.spawn(run_server(sender.clone()));

        let server = Server {
            receiver,
            runtime,
            thread_handle,
        };
        (server, sender)
    }

    pub fn stop(self) {
        let Server {
            runtime,
            thread_handle,
            ..
        } = self;
        thread_handle.abort();
        runtime.shutdown_timeout(Duration::from_secs(3));
    }
}
