use std::{
    io,
    net::{SocketAddr, UdpSocket},
};

use super::{channel::Channel, header::HEADER_SIZE, MAGIC_NUMBER_HEADER};

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
        println!("received packet or length {packet_size}");

        if packet_size >= 4 + HEADER_SIZE && self.buf[..4] == MAGIC_NUMBER_HEADER {
            let data_length = packet_size - HEADER_SIZE - 4;

            self.channel.read(&self.buf[4..packet_size]);

            if data_length > 0 {
                let mut vec = vec![0; packet_size];
                let src = &self.buf[4 + HEADER_SIZE..packet_size];
                vec[..data_length].copy_from_slice(src);
                return Some(vec);
            }
        }
        None
    }
}
