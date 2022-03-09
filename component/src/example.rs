use crate::{IrrecoverableError, Manageable, Supervisor};

use anyhow::anyhow;
use async_trait::async_trait;
use std::net::TcpStream;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot::Receiver as oneshotReceiver;
use tokio::task::JoinHandle;

pub struct Stream {}

impl Stream {
    pub async fn listen(
        stream: TcpStream,
        tx_irrecoverable: Sender<anyhow::Error>,
        rx_cancellation: oneshotReceiver<()>,
    ) {
        tokio::select! {
            _ = async {
                loop {
                    let mut buf = [0; 10];
                    let _len = match stream.peek(&mut buf) {
                        Ok(_) => println!("reading from stream"), // process
                        Err(_) => {
                            let e = anyhow!("missing something required");
                            tx_irrecoverable.send(e).await.expect("Could not send panic signal!🙀 ");
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
        let stream = TcpStream::connect("127.0.0.1:6379").unwrap();
        let handle: JoinHandle<()> =
            tokio::spawn(Self::listen(stream, tx_irrecoverable, rx_cancelllation));
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

// TODO: set that up as an Example http://xion.io/post/code/rust-examples.html
#[tokio::main]
pub async fn main() {
    let stream = Stream {};
    let supervisor = Supervisor::new(stream);
    supervisor.spawn().await.unwrap();
}
