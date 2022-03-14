extern crate component;

use anyhow::anyhow;
use async_trait::async_trait;
use component::{IrrecoverableError, Manageable, Supervisor};
use tokio::net::TcpStream;
use std::thread::sleep;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot::Receiver as oneshotReceiver;
use tokio::task::JoinHandle;

pub struct Stream {
    stream: Option<TcpStream>
}

impl Stream {
    pub fn new() -> Self {
        return Stream{ stream: None,}
    }

    pub async fn connect(mut self) -> Result<(), anyhow::Error> {
        let stream = match TcpStream::connect("127.0.0.1:8080").await {
            Ok(stream) => stream,
            Err(e) => {
                return Result::Err(anyhow!(e))
            }
        };
        self.stream = Option::from(stream);
        Ok(())
    }

    pub async fn listen(
        self,
        tx_irrecoverable: Sender<anyhow::Error>,
        rx_cancellation: oneshotReceiver<()>,
    ) {

        loop {
            let mut buf = [0; 10];
            let _len = match self.stream.peek(&mut buf).await {
                Ok(_) => println!("reading from stream"), // process
                Err(_) => {
                    let e = anyhow!("missing something required");
                    tx_irrecoverable.send(e).await.expect("Could not send irrecoverable signal.");
                    wait_for_cancellation(rx_cancellation).await;
                    return
                }
            };
        }

    }
}

async fn wait_for_cancellation(rx_cancellation: oneshotReceiver<()>) {
    loop {
        tokio::select! {
            _ = rx_cancellation => {
                println!("terminating accept loop");
                break;
            }
        }
    }
}

#[async_trait]
impl Manageable for Stream {
    #[allow(clippy::async_yields_async)]
    async fn start(
        &self,
        tx_irrecoverable: Sender<anyhow::Error>,
        rx_cancellation: oneshotReceiver<()>,
    ) -> tokio::task::JoinHandle<()> {

        let handle: JoinHandle<()> =
            tokio::spawn(self.listen(tx_irrecoverable, rx_cancellation));
        handle
    }

    async fn handle_irrecoverable(
        &self,
        irrecoverable: IrrecoverableError,
    ) -> Result<(), anyhow::Error> {
        println!("Received irrecoverable error {}", irrecoverable);
        Ok(())
    }
}

#[tokio::main]
pub async fn main() -> Result<(), anyhow::Error> {

    let stream = Stream::new();
    stream.connect()?;
    let supervisor = Supervisor::new(stream);
    let _ = match supervisor.spawn().await {
        Ok(_) => {},
        Err(e) => println!("Got this error {:?}", e)
    };
}
