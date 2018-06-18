

#[derive(Debug, Default, Clone)]
pub struct ConnectionPool {}

impl ConnectionPool {
    fn new() -> Self {
        ConnectionPool {}
    }

    fn set_connection(&mut self, url: Url, stream: Stream) {
    }

    fn get_connection(&mut self, url: &Url) -> Option<Stream> {
        None
    }
}

struct KeepAlive {
    url: Url,
    stream: Stream,
    agent: Option<Arc<Mutex<Option<AgentState>>>>,
}

trait ConnectionPoolReturn {
    fn return_to_pool(&self, mut keep_alive: Option<KeepAlive>) {
        //
        let has_agent = keep_alive
            .as_mut()
            .and_then(|k| k.agent.as_mut())
            .map(|a| a.lock().unwrap())
            .map(|l| l.is_some())
            .unwrap_or(false);

        if has_agent {

            // forcefully remove the internal state.
            let keep_alive = keep_alive.take().unwrap();

            // lock mutex.
            let mut state = keep_alive.agent.as_ref().unwrap().lock().unwrap();

            // the agent is here.
            let agent = state.as_mut().unwrap();

            // release connection back to pool.
            agent.pool.set_connection(keep_alive.url, keep_alive.stream);
        }

    }
}