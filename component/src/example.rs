use crate::{IrrecoverableError, Manageable, Supervisor};

use anyhow::anyhow;
use async_trait::async_trait;
use std::net::TcpStream;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot::Receiver as oneshotReceiver;

pub struct Stream {
    stream: TcpStream
}

impl Stream {
    pub fn new() -> Self {
        let stream = TcpStream::connect("127.0.0.1:6379").unwrap();
        Stream { stream }
    }

    pub async fn listen(
        self,
        tx_irrecoverable: Sender<anyhow::Error>,
        rx_cancellation: oneshotReceiver<()>,
    ) {
        tokio::select! {
            _ = async {
                loop {
                    let mut buf = [0; 10];
                    let len = match self.stream.peek(&mut buf) {
                        Ok(_) => println!("reading from stream"), // process
                        Err(_) => {
                            let e = anyhow!("missing something required");
                            tx_irrecoverable.send(e).await;
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
    async fn start(
        &self,
        tx_irrecoverable: Sender<anyhow::Error>,
        rx_cancelllation: oneshotReceiver<()>,
    ) -> tokio::task::JoinHandle<()> {

        let handle = tokio::spawn(self.listen(
            tx_irrecoverable,
            rx_cancelllation,
        ));

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


impl Clone for Stream {
    fn clone(&self) -> Self {
        Stream::new()
    }
}

#[tokio::main]
async fn main() {
    let stream = Stream::new();
    let supervisor = Supervisor::new(stream);
    supervisor.spawn().await;
}
