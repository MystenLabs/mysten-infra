use anyhow::anyhow;
use async_trait::async_trait;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::oneshot::{
    channel as oneshotChannel, Receiver as oneshotReceiver, Sender as oneshotSender,
};

pub type IrrecoverableError = anyhow::Error;
type JoinHandle = tokio::task::JoinHandle<()>;

// The User defines a task launcher, that takes:
// - an irrecoverable error sender, on which the component sends information to the supervisor about
// an irrecoverable event that has ocured
// - a cancellation handle, which will be listened to in the task once an irrecoverable message
// has been sent, used as an "ack" that the message has been received and so the function can return
#[async_trait]
pub trait Manageable {
    // The API for the task launcher
    async fn start(
        &self,
        tx_irrecoverable: Sender<anyhow::Error>,
        rx_cancellation: oneshotReceiver<()>,
    ) -> tokio::task::JoinHandle<()>; // Not the task is "silent" (returns nothing) and does its work through side effects

    // The API for cleanup after the task has encountered an irrecoverable error
    async fn handle_irrecoverable(
        &self,
        irrecoverable: IrrecoverableError,
    ) -> Result<(), anyhow::Error>;
}

pub struct Supervisor<M: Manageable> {
    irrecoverable_signal: Receiver<IrrecoverableError>,
    cancellation_signal: Option<oneshotSender<()>>,
    join_handle: Option<JoinHandle>,
    manageable: M,
}

impl<M: Manageable + Send> Supervisor<M> {
    pub fn new(component: M) -> Self {
        let (_, tr_irrecoverable) = channel(10);
        let (tx_cancellation, _) = oneshotChannel();
        Supervisor {
            irrecoverable_signal: tr_irrecoverable,
            cancellation_signal: Some(tx_cancellation),
            join_handle: None,
            manageable: component,
        }
    }

    // calls th launcher & stores the join handle
    pub async fn spawn(mut self) -> Result<(), anyhow::Error> {
        let (tx_irrecoverable, tr_irrecoverable) = channel(10);
        let (tx_cancellation, tr_cancellation) = oneshotChannel();

        // this assumes the start_fn is populated
        let wrapped_handle = self
            .manageable
            .start(tx_irrecoverable, tr_cancellation)
            .await;

        self.irrecoverable_signal = tr_irrecoverable;
        self.cancellation_signal = Some(tx_cancellation);
        self.join_handle = Some(wrapped_handle);

        self.run().await
    }

    // Run supervision of the child task
    pub async fn run(mut self) -> Result<(), anyhow::Error> {
        // select statement that listens for the following cases:
        //
        // Irrecoverable signal incoming => log, terminate and restart
        // completion of the task => we're done! return
        loop {
            tokio::select! {
                Some(message) = self.irrecoverable_signal.recv() => {
                    self.manageable.handle_irrecoverable(message).await?;
                    self.restart().await?;
                },

               // Poll the JoinHandle<O>
               _result =  self.join_handle.as_mut().unwrap(), if self.join_handle.is_some() => {
                    // this could be due to an un-caught panic
                    // we don't have a user-supplied message to log, so we create a generic one
                    let message = anyhow!("An unexpected shutdown was observed in a component.");
                    self.manageable.handle_irrecoverable(message).await?;
                    self.restart().await?;
                }
            }
        }
    }

    async fn restart(&mut self,) -> Result<(), anyhow::Error> {
        // restart
        let (tx_irrecoverable, tr_irrecoverable) = channel(10);
        let (tx_cancellation, tr_cancellation) = oneshotChannel();

        // call the start method
        let wrapped_handle: JoinHandle =  self.manageable.start(tx_irrecoverable, tr_cancellation).await;
        println!("I have spawned the next component");

        // reset the supervision handles & channel end points
        self.irrecoverable_signal = tr_irrecoverable;
        self.cancellation_signal = Some(tx_cancellation);

        self.join_handle = Some(wrapped_handle);
        Ok(())
    }
}
