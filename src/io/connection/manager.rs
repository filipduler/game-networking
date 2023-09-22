use std::{borrow::BorrowMut, collections::HashMap, net::SocketAddr};

use anyhow::bail;
use crossbeam_channel::Sender;

use crate::io::{int_buffer::IntBuffer, socket::ServerSendPacket, MAGIC_NUMBER_HEADER};

use super::{client::Client, identity::Identity, ConnectionState};

pub struct ConnectionManager {
    capacity: usize,
    active_clients: usize,
    connections: Vec<Option<Client>>,
    addr_map: HashMap<SocketAddr, usize>,
    connect_requests: HashMap<SocketAddr, Identity>,
    sender: Sender<ServerSendPacket>,
}

impl ConnectionManager {
    pub fn new(max_clients: usize, sender: &Sender<ServerSendPacket>) -> Self {
        ConnectionManager {
            capacity: max_clients,
            active_clients: 0,
            addr_map: HashMap::with_capacity(max_clients),
            connections: vec![None::<Client>; max_clients],
            connect_requests: HashMap::new(),
            sender: sender.clone(),
        }
    }

    pub fn get_client_mut(&mut self, addr: &SocketAddr) -> Option<&mut Client> {
        if let Some(connection_index) = self.addr_map.get(addr) {
            if let Some(Some(client_opt)) = self.connections.get_mut(*connection_index) {
                return Some(client_opt);
            }
        }
        None
    }

    pub fn process_connect(
        &mut self,
        addr: &SocketAddr,
        data: &[u8],
    ) -> anyhow::Result<Option<Vec<u8>>> {
        if !self.has_free_slots() {
            return Ok(None);
        }

        let mut buffer = IntBuffer { index: 0 };
        let state = if let Some(state) = ConnectionState::from_repr(buffer.read_u8(data)) {
            state
        } else {
            bail!("invalid connection state in header");
        };

        //check if theres already a connect in process
        //if let Some(identity) = self.connect_requests.get_mut(addr) {
        if let Some(identity) = self.connect_requests.get(addr) {
            if state == ConnectionState::ChallangeResponse
                && identity.session_key == buffer.read_u64(&data)
            {
                return Ok(self.finish_challange(addr));
            }
        } else {
            let client_salt = buffer.read_u64(data);
            let identity = Identity::new(*addr, client_salt, ConnectionState::Challenge);

            self.connect_requests.insert(*addr, identity.clone());

            //generate challange packet
            let mut payload = vec![0_u8; 21];
            buffer.index = 0;

            buffer.write_slice(&MAGIC_NUMBER_HEADER, &mut payload);
            buffer.write_u8(ConnectionState::Challenge as u8, &mut payload);
            buffer.write_u64(client_salt, &mut payload);
            buffer.write_u64(identity.server_salt, &mut payload);

            return Ok(Some(payload));
        }

        Ok(None)
    }

    fn finish_challange(&mut self, addr: &SocketAddr) -> Option<Vec<u8>> {
        if let Some(connection_index) = self.get_free_slot_index() {
            //remove the identity from the connect requests
            if let Some(mut identity) = self.connect_requests.remove(addr) {
                identity.state = ConnectionState::ConnectionAccepted;

                let mut payload = vec![0_u8; 21];
                let mut buffer = IntBuffer { index: 0 };
                buffer.index = 0;

                buffer.write_slice(&MAGIC_NUMBER_HEADER, &mut payload);
                buffer.write_u8(identity.state as u8, &mut payload);
                buffer.write_u32(identity.id, &mut payload);

                //insert the client
                self.insert_client(connection_index, &identity);

                return Some(payload);
            }
        }

        None
    }

    pub fn update(&mut self) {
        for connection in &mut self.connections {
            if let Some(ref mut connection) = connection {
                connection.update();
            }
        }
    }

    fn insert_client(&mut self, index: usize, identity: &Identity) {
        self.connections
            .insert(index, Some(Client::new(identity.clone(), &self.sender)));
        self.active_clients += 1;
    }

    fn has_free_slots(&self) -> bool {
        return self.active_clients < self.capacity;
    }

    fn get_free_slot_index(&self) -> Option<usize> {
        for i in 0..self.capacity {
            if self.connections.get(i).is_none() {
                return Some(i);
            }
        }
        None
    }
}
