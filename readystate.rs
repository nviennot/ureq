#[derive(Clone)]
struct ReadyState(Arc<Mutex<ReadyStateRec>>);

struct ReadyStateRec {
    result: Option<Result<(), Error>>,
    waker: Option<Waker>,
}

impl ReadyStateRec {
    fn new() -> Self {
        ReadyStateRec {
            result: None,
            waker: None,
        }
    }
}

impl ReadyState {
    fn new() -> Self {
        ReadyState(Arc::new(Mutex::new(ReadyStateRec::new())))
    }

    fn mark_ready(&self, res: Result<(), Error>) {
        let mut lock = self.0.lock().unwrap();
        lock.result = Some(res);
        if let Some(waker) = lock.waker.take() {
            waker.wake();
        }
    }
}

impl Future for ReadyState {
    type Output = Result<(), Error>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut lock = self.0.lock().unwrap();
        if let Some(res) = lock.result.take() {
            res.into()
        } else {
            lock.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
