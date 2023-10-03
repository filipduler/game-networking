use std::{borrow::BorrowMut, collections::HashMap, net::SocketAddr, sync::Arc};

use anyhow::bail;
use crossbeam_channel::Sender;

use crate::net::{
    array_pool::ArrayPool, int_buffer::IntBuffer, socket::UdpSendEvent, PacketType,
    MAGIC_NUMBER_HEADER,
};

use super::{identity::Identity, Connection};

pub struct ConnectionManager {
    capacity: usize,
    active_clients: usize,
    connections: Vec<Option<Connection>>,
    addr_map: HashMap<SocketAddr, usize>,
    connect_requests: HashMap<SocketAddr, Identity>,
    sender: Sender<UdpSendEvent>,
    array_pool: Arc<ArrayPool>,
}

impl ConnectionManager {
    pub fn new(
        max_clients: usize,
        sender: &Sender<UdpSendEvent>,
        array_pool: &Arc<ArrayPool>,
    ) -> Self {
        ConnectionManager {
            capacity: max_clients,
            active_clients: 0,
            addr_map: HashMap::with_capacity(max_clients),
            connections: (0..max_clients).map(|_| None).collect(),
            connect_requests: HashMap::new(),
            sender: sender.clone(),
            array_pool: array_pool.clone(),
        }
    }

    pub fn get_client_mut(&mut self, addr: &SocketAddr) -> Option<&mut Connection> {
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
        let state = if let Some(state) = PacketType::from_repr(buffer.read_u8(data)) {
            state
        } else {
            bail!("invalid connection state in header");
        };

        //check if theres already a connect in process
        //if let Some(identity) = self.connect_requests.get_mut(addr) {
        if let Some(identity) = self.connect_requests.get(addr) {
            if state == PacketType::ChallangeResponse
                && identity.session_key == buffer.read_u64(data)
            {
                return Ok(self.finish_challange(addr));
            }
        } else {
            let client_salt = buffer.read_u64(data);
            let identity = Identity::new(*addr, client_salt);

            self.connect_requests.insert(*addr, identity.clone());

            //generate challange packet
            let mut payload = vec![0_u8; 21];
            buffer.index = 0;

            buffer.write_slice(&MAGIC_NUMBER_HEADER, &mut payload);
            buffer.write_u8(PacketType::Challenge as u8, &mut payload);
            buffer.write_u64(client_salt, &mut payload);
            buffer.write_u64(identity.server_salt, &mut payload);

            return Ok(Some(payload));
        }

        Ok(None)
    }

    fn finish_challange(&mut self, addr: &SocketAddr) -> Option<Vec<u8>> {
        if let Some(connection_index) = self.get_free_slot_index() {
            //remove the identity from the connect requests
            if let Some(identity) = self.connect_requests.remove(addr) {
                let mut payload = vec![0_u8; 21];
                let mut buffer = IntBuffer { index: 0 };
                buffer.index = 0;

                buffer.write_slice(&MAGIC_NUMBER_HEADER, &mut payload);
                buffer.write_u8(PacketType::ConnectionAccepted as u8, &mut payload);
                buffer.write_u32(identity.id, &mut payload);

                //insert the client
                self.insert_connection(connection_index, &identity);

                return Some(payload);
            }
        }

        None
    }

    pub fn update(&mut self) {
        for connection in self.connections.iter_mut().flatten() {
            connection.update();
        }
    }

    fn insert_connection(&mut self, index: usize, identity: &Identity) {
        self.connections.insert(
            index,
            Some(Connection::new(
                identity.clone(),
                &self.sender,
                &self.array_pool,
            )),
        );
        self.addr_map.insert(identity.addr, index);
        self.active_clients += 1;
    }

    pub fn disconnect_connection(&mut self, addr: SocketAddr) {
        if let Some(index) = self.addr_map.get(&addr).cloned() {
            let slot = &self.connections[index];
            if slot.is_some() {
                self.active_clients -= 1;
            }
            self.addr_map.remove(&addr);
            self.connections[index] = None;
        }
    }

    fn has_free_slots(&self) -> bool {
        self.active_clients < self.capacity
    }

    fn get_free_slot_index(&self) -> Option<usize> {
        (0..self.capacity).find(|&i| self.connections.get(i).unwrap().is_none())
    }
}
