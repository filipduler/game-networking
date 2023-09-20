use std::{
    net::SocketAddr,
    sync::atomic::{AtomicU32, Ordering},
    time::Instant,
};

use crossbeam_channel::Sender;
use rand::Rng;

use super::{
    channel::Channel,
    header::{Header, SendType, HEADER_SIZE},
    send_buffer::SendBufferManager,
    sequence_buffer::SequenceBuffer,
    BUFFER_SIZE, MAGIC_NUMBER_HEADER,
};
static CONNECTION_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

pub struct Connection {
    pub identity: Identity,
    pub unreliable_local_seq: u32,
    pub reliable_channel: Channel,
    pub received_at: Instant,
    pub last_received: Instant,
    sender: Sender<(SocketAddr, u32, Vec<u8>)>,
}

impl Connection {
    pub fn new(
        addr: SocketAddr,
        sender: &Sender<(SocketAddr, u32, Vec<u8>)>,
        client_salt: u64,
    ) -> Self {
        let server_salt = rand::thread_rng().gen();

        Self {
            identity: Identity {
                id: CONNECTION_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
                addr,
                client_salt,
                server_salt,
                session_key: client_salt ^ server_salt,
                accepted: false,
            },
            unreliable_local_seq: 0,
            reliable_channel: Channel::new(),
            received_at: Instant::now(),
            last_received: Instant::now(),
            sender: sender.clone(),
        }
    }

    pub fn send_reliable(&mut self, data: Option<&[u8]>) {
        let send_buffer = self.reliable_channel.create_send_buffer(data);
        self.sender
            .send((
                self.identity.addr,
                send_buffer.seq,
                send_buffer.data.to_vec(),
            ))
            .unwrap();
        self.reliable_channel.send_ack = false;
    }

    pub fn send_unreliable(&mut self, data: Option<&[u8]>) {
        let mut header = Header::new(self.unreliable_local_seq, SendType::Unreliable);
        self.reliable_channel.write_header_ack_fiels(&mut header);

        let payload = Header::create_packet(&header, data);

        self.unreliable_local_seq += 1;
        self.reliable_channel.send_ack = false;

        self.sender
            .send((self.identity.addr, header.seq, payload))
            .unwrap();
    }

    pub fn update(&mut self) {
        let resend_packets = self.reliable_channel.get_redelivery_packet();
        for packet in resend_packets {
            let mut header = Header::new(packet.seq, SendType::Reliable);
            self.reliable_channel.write_header_ack_fiels(&mut header);

            let payload = Header::create_packet(&header, Some(&packet.data));

            self.sender
                .send((self.identity.addr, packet.seq, payload))
                .unwrap();

            self.reliable_channel.send_ack = false;
        }

        if self.reliable_channel.send_ack {
            self.send_unreliable(None);
        }
    }
}

pub struct Identity {
    pub id: u32,
    pub addr: SocketAddr,
    pub client_salt: u64,
    pub server_salt: u64,
    pub session_key: u64,
    pub accepted: bool,
}
