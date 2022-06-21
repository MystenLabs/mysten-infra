use crate::MetricsCallbackProvider;
use pin_project::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;
use tonic::codegen::http::{Request, Response};
use tower::{Layer, Service};

#[derive(Clone)]
pub struct RequestMetricsLayer<M: MetricsCallbackProvider> {
    metrics_callback: Arc<M>,
}

impl<M: MetricsCallbackProvider> RequestMetricsLayer<M> {
    pub fn new(metrics_callback: Arc<M>) -> Self {
        RequestMetricsLayer { metrics_callback }
    }
}

impl<S, M: MetricsCallbackProvider> Layer<S> for RequestMetricsLayer<M> {
    type Service = RequestMetrics<S, M>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestMetrics::new(inner, self.metrics_callback.clone())
    }
}

#[derive(Clone)]
pub struct RequestMetrics<S, M: MetricsCallbackProvider> {
    inner: S,
    metrics_callback: Arc<M>,
}

impl<S, M: MetricsCallbackProvider> RequestMetrics<S, M> {
    pub fn new(inner: S, metrics_callback: Arc<M>) -> Self {
        RequestMetrics {
            inner,
            metrics_callback,
        }
    }
}

impl<S, ReqBody, ResBody, M: MetricsCallbackProvider> Service<Request<ReqBody>>
    for RequestMetrics<S, M>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = ResponseFuture<S::Future, M>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let start_time = Instant::now();
        let path = req.uri().path().to_owned();

        ResponseFuture {
            response_future: self.inner.call(req),
            path,
            start_time,
            metrics_callback: self.metrics_callback.clone(),
        }
    }
}

#[pin_project]
pub struct ResponseFuture<F, M> {
    #[pin]
    response_future: F,
    #[pin]
    path: String,
    #[pin]
    start_time: Instant,
    #[pin]
    metrics_callback: Arc<M>,
}

impl<F, ResBody, E, M: MetricsCallbackProvider> Future for ResponseFuture<F, M>
where
    F: Future<Output = Result<Response<ResBody>, E>>,
{
    type Output = Result<Response<ResBody>, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        match this.response_future.poll(cx) {
            Poll::Ready(Ok(res)) => {
                let start_time = this.start_time;
                let status = res.status().as_u16();
                let path = this.path.to_string();

                this.metrics_callback.on_request(path, *start_time, status);

                Poll::Ready(Ok(res))
            }
            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Pending => Poll::Pending,
        }
    }
}
