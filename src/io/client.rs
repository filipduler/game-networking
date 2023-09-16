use std::{
    io,
    net::{SocketAddr, UdpSocket},
};

use super::channel::Channel;

pub struct Client {
    socket: UdpSocket,
    channel: Channel,
}

impl Client {
    pub fn connect(local_addr: SocketAddr, remote_addr: SocketAddr) -> io::Result<Self> {
        let socket = UdpSocket::bind(local_addr)?;
        socket.connect(remote_addr)?;

        Ok(Client {
            socket,
            channel: Channel::new(),
        })
    }

    pub fn send(&mut self, data: Vec<u8>) -> io::Result<()> {
        let send_buffer = self.channel.send(Some(&data));
        self.socket.send(&send_buffer.data)?;

        let mut buf = [0_u8; 1 << 16];
        self.socket.recv(&mut buf)?;

        Ok(())
    }
}
