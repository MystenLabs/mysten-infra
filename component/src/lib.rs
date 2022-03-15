use anyhow::anyhow;
use async_trait::async_trait;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::oneshot::{
    channel as oneshotChannel, Receiver as oneshotReceiver, Sender as oneshotSender,
};

pub type IrrecoverableError = anyhow::Error;
type JoinHandle = tokio::task::JoinHandle<()>;

/// A Supervisor is instantiated to supervise a task that should always be running.
/// A running supervisor will start a component task, and ensure that it is restarted
/// if it ever stops.
pub struct Supervisor<M: Manageable> {
    irrecoverable_signal: Receiver<IrrecoverableError>,
    cancellation_signal: Option<oneshotSender<()>>,
    join_handle: Option<JoinHandle>,
    manageable: M,
}

/// In order to be Manageable, a user defines the following two functions:
///
/// 1. A start function that launches a tokio task, as input it takes:
/// - an irrecoverable error sender, on which the component sends information to the supervisor about
/// an irrecoverable event that has occurred
/// - a cancellation handle, which will be listened to in the task once an irrecoverable message
/// has been sent, used as an "ack" that the message has been received and so the function can return
///
/// 2. A handle_irrecoverable which takes actions on a relaunch due to an irrecoverable error
/// that happened. It takes the error message that may contain a stack trace and other information
/// that was sent to the Supervisor via the tx_irrecoverable passed into start.
#[async_trait]
pub trait Manageable {
    // The function that spawns a tokio task
    async fn start(
        &self,
        tx_irrecoverable: Sender<anyhow::Error>,
        rx_cancellation: oneshotReceiver<()>,
    ) -> tokio::task::JoinHandle<()>; // Note the task is "silent" (returns nothing)

    // The function for cleanup after the task has encountered an irrecoverable error
    fn handle_irrecoverable(&self, irrecoverable: IrrecoverableError) -> Result<(), anyhow::Error>;
}

impl<M: Manageable + Send> Supervisor<M> {
    /// Creates a new supervisor using a Manageable component.
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

    /// Spawn calls the start function of the Manageable component and runs supervision.
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

    /// Run watches continuously for irrecoverable errors or JoinHandle completion.
    pub async fn run(mut self) -> Result<(), anyhow::Error> {
        // select statement that listens for the following cases:
        //
        // Irrecoverable signal incoming => log, terminate and restart
        // completion of the task => already terminated, log and restart
        //
        // The handle_irrecoverable is run before the existing task gets
        // cancelled by restart in the case that an irrecoverable signal
        // was sent to us. This makes resource cleanup possible.
        loop {
            tokio::select! {
                Some(message) = self.irrecoverable_signal.recv() => {
                    self.manageable.handle_irrecoverable(message)?;
                    self.restart().await?;
                },

               // Poll the JoinHandle<O>
               _result =  self.join_handle.as_mut().unwrap(), if self.join_handle.is_some() => {
                    // this could be due to an un-caught panic
                    // we don't have a user-supplied message to log, so we create a generic one
                    let message = anyhow!("An unexpected shutdown was observed in a component.");
                    self.manageable.handle_irrecoverable(message)?;
                    self.restart().await?;
                }
            }
        }
    }

    async fn restart(&mut self) -> Result<(), anyhow::Error> {
        // restart
        let (tx_irrecoverable, tr_irrecoverable) = channel(10);
        let (tx_cancellation, tr_cancellation) = oneshotChannel();

        // call the start method
        let wrapped_handle: JoinHandle = self
            .manageable
            .start(tx_irrecoverable, tr_cancellation)
            .await;

        // reset the supervision handles & channel end points
        // dropping the old cancellation_signal implicitly sends cancellation by closing the channel
        self.irrecoverable_signal = tr_irrecoverable;
        self.cancellation_signal = Some(tx_cancellation);

        self.join_handle = Some(wrapped_handle);
        Ok(())
    }
}
