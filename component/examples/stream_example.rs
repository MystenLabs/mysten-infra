extern crate component;

use anyhow::anyhow;
use async_trait::async_trait;
use component::{IrrecoverableError, Manageable, Supervisor};
use std::net::TcpStream;
use std::thread::sleep;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot::Receiver as oneshotReceiver;
use tokio::task::JoinHandle;

pub struct Stream {}

impl Stream {
    pub async fn listen(
        tx_irrecoverable: Sender<anyhow::Error>,
        rx_cancellation: oneshotReceiver<()>,
    ) {

        let stream = match TcpStream::connect("127.0.0.1:8080") {
            Ok(stream) => stream,
            Err(e) => {
                tx_irrecoverable.send(anyhow!(e))
                    .await.expect("Could not send irrecoverable signal.");
                return;
            }
        };

        tokio::select! {
            _ = async {
                loop {
                    let mut buf = [0; 10];
                    let _len = match stream.peek(&mut buf) {
                        Ok(_) => println!("reading from stream"), // process
                        Err(_) => {
                            let e = anyhow!("missing something required");
                            tx_irrecoverable.send(e).await.expect("Could not send irrecoverable signal.");
                        }
                    } ;
                }

            } => {}
            _ = rx_cancellation => {
                println!("terminating accept loop");
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
        rx_cancelllation: oneshotReceiver<()>,
    ) -> tokio::task::JoinHandle<()> {

        let handle: JoinHandle<()> =
            tokio::spawn(Self::listen(tx_irrecoverable, rx_cancelllation));
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
pub async fn main() {

    let stream = Stream {};
    let supervisor = Supervisor::new(stream);
    let _ = match supervisor.spawn().await {
        Ok(_) => {},
        Err(e) => println!("Got this error {:?}", e)
    };
    sleep( Duration::from_secs(1));
}

