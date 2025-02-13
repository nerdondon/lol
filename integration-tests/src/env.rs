use super::node::*;

use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::{Arc, RwLock};
use std::time::Duration;

#[derive(Clone)]
pub struct NodeCommand {
    name: String,
    pub args: Vec<String>,
}
impl NodeCommand {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            args: vec![],
        }
    }
    pub fn with_args<'a, I: IntoIterator<Item = &'a str>>(self, args: I) -> Self {
        Self {
            name: self.name,
            args: args.into_iter().map(|x| x.to_owned()).collect(),
        }
    }
}
pub struct Environment {
    port_list: RwLock<HashMap<u8, u16>>,
    node_list: RwLock<HashMap<u8, Node>>,
}
fn available_port() -> std::io::Result<u16> {
    TcpListener::bind("localhost:0").map(|x| x.local_addr().unwrap().port())
}
impl Environment {
    /// make a cluster of only one node
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            port_list: RwLock::new(HashMap::new()),
            node_list: RwLock::new(HashMap::new()),
        })
    }
    pub fn get_node_id(&self, id: u8) -> String {
        let port_list = self.port_list.read().unwrap();
        let port = port_list.get(&id).unwrap();
        // the node id should be canonicalized to match the uri by the strings.
        format!("http://localhost:{}/", port)
    }
    pub fn start(&self, id: u8, command: NodeCommand) {
        let mut port_list = self.port_list.write().unwrap();
        let port = match port_list.get(&id) {
            Some(&port) => port,
            None => {
                let newport = available_port().unwrap();
                port_list.insert(id, newport);
                newport
            }
        };
        // the command must take socket addr at the first parameter
        // followed by the rest of the parameters.

        // Id can be host:port. it is resolved by the server.
        let prof_file = format!("cov/{}.profraw", crate::get_seqid());
        let child = std::process::Command::new(command.name)
            .env("LLVM_PROFILE_FILE", prof_file)
            .arg(&format!("http://localhost:{}", port))
            .args(command.args)
            .spawn()
            .expect(&format!("failed to start node id={}", id));

        let mut node_list = self.node_list.write().unwrap();
        node_list.insert(id, Node { child });
    }
    pub fn stop(&self, id: u8) {
        let mut node_list = self.node_list.write().unwrap();
        node_list.remove(&id);
    }
    pub fn pause(&self, id: u8) {
        let mut node_list = self.node_list.write().unwrap();
        node_list.get_mut(&id).unwrap().pause();
    }
    pub fn unpause(&self, id: u8) {
        let mut node_list = self.node_list.write().unwrap();
        node_list.get_mut(&id).unwrap().unpause();
    }
}
impl Drop for Environment {
    fn drop(&mut self) {
        let ids = {
            let node_list = self.node_list.read().unwrap();
            node_list.keys().cloned().collect::<Vec<_>>()
        };
        for id in ids {
            self.stop(id);
        }
    }
}
/// Wait for some consensus to be a value computed by `f`.
pub fn wait_for_consensus<T: Eq + Clone>(
    timeout: Duration,
    nodes: Vec<u8>,
    f: impl Fn(u8) -> Option<T>,
) -> Option<T> {
    use std::time::Instant;
    let mut remaining = timeout;
    let mut ok = false;
    let mut ret = None;
    while !ok {
        let start = Instant::now();
        let mut rr = vec![];
        for &id in &nodes {
            let r = f(id);
            rr.push(r);
        }
        ok = true;
        let fi = &rr[0];
        for r in &rr {
            if r.is_none() {
                ok = false;
            }
            if r != fi {
                ok = false;
            }
        }
        if ok {
            let r = rr[0].clone();
            ret = r;
            break;
        }
        let end = Instant::now();
        let elapsed = end - start;
        if remaining < elapsed {
            ret = None;
            break;
        } else {
            remaining -= elapsed;
        }
    }
    ret
}

/// Wait for some value computed by `f` to be `should_be`.
pub fn eventually<T: Eq>(timeout: Duration, should_be: T, f: impl Fn() -> T) -> bool {
    use std::time::Instant;
    let mut remaining = timeout;
    loop {
        let start = Instant::now();
        let res = f();
        if res == should_be {
            return true;
        }
        let end = Instant::now();
        let elapsed = end - start;
        if remaining < elapsed {
            return false;
        }
        remaining -= elapsed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wait_for_consensus() {
        let mut nodes = vec![];
        for i in 0..100 {
            nodes.push(i);
        }

        let r = wait_for_consensus(Duration::from_secs(1), nodes.clone(), |id| Some(id));
        assert_eq!(r, None);
        let r = wait_for_consensus(Duration::from_secs(1), nodes.clone(), |_| Some(10));
        assert_eq!(r, Some(10));
        let r = wait_for_consensus(Duration::from_secs(1), nodes.clone(), |_| {
            None as Option<u8>
        });
        assert_eq!(r, None);
        let r = wait_for_consensus(Duration::from_secs(1), nodes.clone(), |id| match id {
            0 => Some(0),
            _ => None,
        });
        assert_eq!(r, None);
    }
}
