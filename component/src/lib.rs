use futures::future;
use tokio::{sync::mpsc::{channel, Sender, Receiver}};
use tokio::sync::oneshot::{channel as oneshotChannel, Sender as oneshotSender, Receiver as oneshotReceiver};
use tokio::time::{Duration, timeout};

type IrrecoverableError = std::io::Error;
type TokioJoinHande = tokio::task::JoinHandle<()>;

pub trait Manageable {
    fn start(&self, _: Sender<IrrecoverableError>, _: oneshotReceiver<()>) -> future::BoxFuture<'static, ()>;

}

pub struct Component {
    cancel_signal: oneshotSender<()>,
    panic_signal: Receiver<IrrecoverableError>,
    join_handle: TokioJoinHande,
    start_fn : Box<dyn Manageable>,
}

impl Component {
    pub async fn spawn<M>(&mut self, starter: M)
    where M: Manageable + Send + Copy + 'static
    {
        let (panic_sender, panic_receiver) = channel(10);
        let (cancel_sender, cancel_receiver) = oneshotChannel();
        let wrapped_handle = tokio::spawn(starter.start(panic_sender, cancel_receiver));
        self.panic_signal = panic_receiver;
        self.cancel_signal = cancel_sender;
        self.join_handle = wrapped_handle;
        self.start_fn = Box::new(starter);
    }

    pub async fn join(self) -> Result<(), std::io::Error> {
        self.join_handle.await?;
        Ok(())
    }

    pub async fn terminate(mut self) -> Result<(), std::io::Error> {
        self.cancel_signal.send(()).unwrap();

        if let Err(_) = timeout(Duration::from_secs(2), &mut self.join_handle).await {
            self.join_handle.abort();
        }

        Ok(())
    }

    pub async fn restart(mut self) {
        let (panic_sender, panic_receiver) = channel(10);
        let (cancel_sender, cancel_receiver) = oneshotChannel();
        let wrapped_handle = tokio::spawn(self.start_fn.start(panic_sender, cancel_receiver));
        self.panic_signal = panic_receiver;
        self.cancel_signal = cancel_sender;
        self.join_handle = wrapped_handle;
    }

}



#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
