use failure::err_msg;
use futures::prelude::*;
use gu_client::r#async::{Peer, ProviderRef};
use gu_hardware::actor::{Hardware, HardwareQuery};

/// Query hardware of a peer
///
/// Hardware query is not an official part of the GU API, so we make use of RPC.
pub trait PeerHardwareQuery {
    fn hardware(&self) -> Box<dyn Future<Item = Hardware, Error = failure::Error>>;
}

impl PeerHardwareQuery for Peer {
    fn hardware(&self) -> Box<dyn Future<Item = Hardware, Error = failure::Error>> {
        let provider: ProviderRef = self.clone().into();
        let future = provider
            .rpc_call(HardwareQuery)
            .from_err()
            .and_then(|reply| reply.map_err(err_msg).into_future());
        Box::new(future)
    }
}
