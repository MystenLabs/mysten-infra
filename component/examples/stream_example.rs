extern crate component;

use std::cmp::min;
use std::sync::Once;
use anyhow::anyhow;
use async_trait::async_trait;
use component::{IrrecoverableError, Manageable, Supervisor};
use tokio::net::TcpStream;
use std::thread::sleep;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot::Receiver as oneshotReceiver;
use tokio::task::JoinHandle;

/// We create two structs, an empty struct as a component that will contain functions
/// that create and fully encapsulate an instance of the actual type.
/// An instance of the actual type needs to be instantiated inside the supervised task
/// in order to have the correct lifetime.
pub struct MockTcpStreamComponent {}

pub struct MockTcpStream {
    read_data: Vec<u8>,
    write_data: Vec<u8>,
    failure: Once,
}

impl MockTcpStream {
    pub fn new() -> Self {
        let read_data= Vec::new();
        let write_data = Vec::new();
        let failure = Once::new();
        MockTcpStream {
            read_data,
            write_data,
            failure,
        }
    }

    fn mock_read(&self, buf: &mut [u8]) -> Result<usize, anyhow::Error> {
        // failure
        let mut f = false;
        self.failure.call_once(|| {
            f = true;
        });
        if f {
            return Result::Err(anyhow!("Could not read from stream."))
        }

        let size: usize = min(self.read_data.len(), buf.len());
        buf[..size].copy_from_slice(&self.read_data[..size]);
        Ok(size)
    }
}

impl MockTcpStreamComponent {
    pub async fn listen(
        tx_irrecoverable: Sender<anyhow::Error>,
        rx_cancellation: oneshotReceiver<()>,
    ) {
        // Initialize the concrete type
        let m_tcp = MockTcpStream::new();

        loop {
            let mut buf = [0; 10];
            let _len = match m_tcp.mock_read(&mut buf) {
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
impl Manageable for MockTcpStreamComponent {
    #[allow(clippy::async_yields_async)]
    async fn start(
        &self,
        tx_irrecoverable: Sender<anyhow::Error>,
        rx_cancellation: oneshotReceiver<()>,
    ) -> tokio::task::JoinHandle<()> {


        let handle: JoinHandle<()> =
            tokio::spawn(Self::listen(tx_irrecoverable, rx_cancellation));
        handle
    }

    async fn handle_irrecoverable(
        &self,
        irrecoverable: IrrecoverableError,
    ) -> Result<(), anyhow::Error> {
        println!("Received irrecoverable error: {}", irrecoverable);
        Ok(())
    }
}

#[tokio::main]
pub async fn main() -> Result<(), anyhow::Error> {

    let stream_component = MockTcpStreamComponent {};
    let supervisor = Supervisor::new(stream_component);
    let _ = match supervisor.spawn().await {
        Ok(_) => {},
        Err(e) => println!("Got this error {:?}", e)
    };
    Ok(())
}
