use std::{
    io,
    net::{SocketAddr, UdpSocket},
};

use super::{
    channel::Channel,
    header::{Header, HEADER_SIZE},
    MAGIC_NUMBER_HEADER,
};

pub struct Client {
    socket: UdpSocket,
    channel: Channel,
    buf: [u8; 1 << 16],
}

impl Client {
    pub fn connect(local_addr: SocketAddr, remote_addr: SocketAddr) -> io::Result<Self> {
        let socket = UdpSocket::bind(local_addr)?;
        socket.connect(remote_addr)?;

        Ok(Client {
            socket,
            channel: Channel::new(),
            buf: [0_u8; 1 << 16],
        })
    }

    pub fn send(&mut self, data: Vec<u8>) -> io::Result<()> {
        let send_buffer = self.channel.create_send_buffer(Some(&data));
        self.socket.send(&send_buffer.data)?;

        Ok(())
    }

    pub fn read(&mut self) -> Option<Vec<u8>> {
        let packet_size = self.socket.recv(&mut self.buf).unwrap();
        if packet_size >= 4 && self.buf[..4] == MAGIC_NUMBER_HEADER {
            let data_length = packet_size - HEADER_SIZE - 4;
            let header = Header::read(&self.buf[4..packet_size]);
            //TODO: check if its duplicate

            //mark the packet as recieved
            self.channel.update_remote_seq(header.seq);
            self.channel.mark_sent_packets(header.ack, header.ack_bits);

            //send ack
            self.channel.send_ack = true;
            if data_length > 0 {
                let mut vec = Vec::with_capacity(packet_size - HEADER_SIZE - 4);
                vec.copy_from_slice(&self.buf[4 + HEADER_SIZE..packet_size]);

                return Some(vec);
            }
        }
        None
    }
}
