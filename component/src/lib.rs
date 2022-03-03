use futures::future;
use tokio::signal;
use tokio::{sync::mpsc::{channel, Sender, Receiver}};
use tokio::sync::oneshot::{channel as oneshotChannel, Sender as oneshotSender, Receiver as oneshotReceiver};
use tokio::time::{Duration, timeout};


type IrrecoverableError = std::io::Error;
type TokioJoinHande = tokio::task::JoinHandle<()>;

pub trait Manageable {
    fn start(&self, _: Sender<IrrecoverableError>, _: oneshotReceiver<()>) -> future::BoxFuture<'static, ()>;

}

pub struct Component {
    shutdown_signal: oneshotSender<()>,
    panic_signal: Receiver<IrrecoverableError>,
    join_handle: TokioJoinHande,
    start_fn : Box<dyn Manageable>,
}

impl Component{
    pub async fn join(self) -> Result<(), std::io::Error> {
        self.join_handle.await?;
        Ok(())
    }
}


pub async fn spawn<M>(starter: M) -> Component
where M: Manageable + Send + Copy + 'static
{
    let (panic_sender, panic_receiver) = channel(10);
    let (cancel_sender, cancel_receiver) = oneshotChannel();
    let wrapped_handle = tokio::spawn( starter.start(panic_sender, cancel_receiver));
    Component{panic_signal: panic_receiver,
        shutdown_signal: cancel_sender,
        join_handle: wrapped_handle,
        start_fn: Box::new(starter),
    }
}

pub async fn run_supervision(mut c: Component) ->Result<(), std::io::Error> {
    // first, call join to start the future
    //
    // select statement that listens for the following cases:
    // panic signal incoming => log, terminate and restart
    // ctrl + c signal => send shutdown signal
    //
    loop {
        tokio::select! {
            message = c.panic_signal.recv() => {
                println!("panic message received: {:?}", message);
                terminate(c.shutdown_signal, c.join_handle).await?;
                // restart
                let (panic_sender, panic_receiver) = channel(10);
                let (cancel_sender, cancel_receiver) = oneshotChannel();
                let wrapped_handle = tokio::spawn(c.start_fn.start(panic_sender, cancel_receiver));
                c.panic_signal = panic_receiver;
                c.shutdown_signal = cancel_sender;
                c.join_handle = wrapped_handle;
            }


            _ = signal::ctrl_c() => {
                terminate(c.shutdown_signal, c.join_handle).await?;
                return Ok(())
            }
        }
    }

}

pub async fn terminate(shutdown_sender: oneshotSender<()>, mut join_handle: TokioJoinHande) -> Result<(), std::io::Error> {
    shutdown_sender.send(()).unwrap();

    if let Err(_) = timeout(Duration::from_secs(2), &mut join_handle).await {
        join_handle.abort();
    }
    Ok(())
}
