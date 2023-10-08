use std::{
    borrow::BorrowMut,
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    rc::Rc,
    sync::Arc,
};

use anyhow::bail;
use crossbeam_channel::Sender;

pub enum ConnectionStatus {
    Rejected,
    Connecting,
    Connected(u32),
}

use crate::net::{
    array_pool::{ArrayPool, BufferPoolRef},
    int_buffer::IntBuffer,
    send_buffer::SendPayload,
    socket::UdpSendEvent,
    PacketType, MAGIC_NUMBER_HEADER,
};

use super::{identity::Identity, Connection};

pub struct ConnectionManager {
    capacity: usize,
    active_clients: usize,
    connections: Vec<Option<Connection>>,
    addr_map: HashMap<SocketAddr, usize>,
    connect_requests: HashMap<SocketAddr, Identity>,
    marked_packets_buf: Vec<Rc<SendPayload>>,
}

impl ConnectionManager {
    pub fn new(max_clients: usize) -> Self {
        ConnectionManager {
            capacity: max_clients,
            active_clients: 0,
            addr_map: HashMap::with_capacity(max_clients),
            connections: (0..max_clients).map(|_| None).collect(),
            connect_requests: HashMap::new(),
            marked_packets_buf: Vec::new(),
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
        buffer: BufferPoolRef,
        send_queue: &mut VecDeque<UdpSendEvent>,
    ) -> anyhow::Result<ConnectionStatus> {
        if !self.has_free_slots() {
            return Ok(ConnectionStatus::Rejected);
        }

        let mut int_buffer = IntBuffer::default();
        let state = if let Some(state) = PacketType::from_repr(int_buffer.read_u8(&buffer)) {
            state
        } else {
            bail!("invalid connection state in header");
        };

        //check if theres already a connect in process
        //if let Some(identity) = self.connect_requests.get_mut(addr) {
        if let Some(identity) = self.connect_requests.get(addr) {
            if state == PacketType::ChallengeResponse
                && identity.session_key == int_buffer.read_u64(&buffer)
            {
                let client_id = identity.id;
                if let Some(buffer) = self.finish_challenge(addr) {
                    send_queue.push_back(UdpSendEvent::Server(buffer, *addr));
                    return Ok(ConnectionStatus::Connected(client_id));
                }
            }
        } else {
            let client_salt = int_buffer.read_u64(&buffer);
            let identity = Identity::new(*addr, client_salt);

            self.connect_requests.insert(*addr, identity.clone());

            //generate challenge packet
            let mut buffer = ArrayPool::rent(21);
            int_buffer.reset();

            int_buffer.write_slice(&MAGIC_NUMBER_HEADER, &mut buffer);
            int_buffer.write_u8(PacketType::Challenge as u8, &mut buffer);
            int_buffer.write_u64(client_salt, &mut buffer);
            int_buffer.write_u64(identity.server_salt, &mut buffer);

            send_queue.push_back(UdpSendEvent::Server(buffer, *addr));
            return Ok(ConnectionStatus::Connecting);
        }

        Ok(ConnectionStatus::Rejected)
    }

    fn finish_challenge(&mut self, addr: &SocketAddr) -> Option<BufferPoolRef> {
        if let Some(connection_index) = self.get_free_slot_index() {
            //remove the identity from the connect requests
            if let Some(identity) = self.connect_requests.remove(addr) {
                let mut buffer = ArrayPool::rent(9);
                let mut int_buffer = IntBuffer::default();

                int_buffer.write_slice(&MAGIC_NUMBER_HEADER, &mut buffer);
                int_buffer.write_u8(PacketType::ConnectionAccepted as u8, &mut buffer);
                int_buffer.write_u32(identity.id, &mut buffer);

                //insert the client
                self.insert_connection(connection_index, &identity);

                return Some(buffer);
            }
        }

        None
    }

    pub fn update(&mut self, send_queue: &mut VecDeque<UdpSendEvent>) {
        for connection in self.connections.iter_mut().flatten() {
            connection.update(&mut self.marked_packets_buf, send_queue);
        }
    }

    fn insert_connection(&mut self, index: usize, identity: &Identity) {
        self.connections
            .insert(index, Some(Connection::new(identity.clone())));
        self.addr_map.insert(identity.addr, index);
        self.active_clients += 1;
    }

    pub fn disconnect_connection(&mut self, addr: SocketAddr) -> Option<u32> {
        let mut client_id = None;

        if let Some(index) = self.addr_map.get(&addr).cloned() {
            let slot = &self.connections[index];
            if let Some(connection) = slot {
                self.active_clients -= 1;
                client_id = Some(connection.identity.id);
            }
            self.addr_map.remove(&addr);
            self.connections[index] = None;
        }

        client_id
    }

    fn has_free_slots(&self) -> bool {
        self.active_clients < self.capacity
    }

    fn get_free_slot_index(&self) -> Option<usize> {
        (0..self.capacity).find(|&i| self.connections.get(i).unwrap().is_none())
    }
}
