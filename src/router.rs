use std::sync::Arc;

#[derive(Clone)]
pub struct Route {
    pattern: String,
    handler: Arc<dyn Fn(&str) -> tokio::task::JoinHandle<String> + Send + Sync>,
}

#[derive(Clone)]
pub struct Router {
    routes: Vec<Route>,
}

impl Router {
    pub fn new() -> Self {
        Router { routes: Vec::new() }
    }

    pub fn add_route<F, Fut>(&mut self, pattern: String, handler: F)
    where 
        F: Fn(String) -> Fut + 'static + Send + Sync,
        Fut: std::future::Future<Output = String> + Send + 'static,
    {
        self.routes.push(
            Route {
                pattern,
                handler: Arc::new(move |id| {
                    tokio::spawn(handler(id.to_string()))
                }),
            }
        );
    }

    pub async fn handle(&self, path: &str) -> Option<String> {
        for route in &self.routes {
            if path.starts_with(&route.pattern) {
                let param = path.replace(&route.pattern, "");
                let handle = (route.handler)(&param);
                return Some(handle.await.unwrap());
            }
        }
        None
    }
}