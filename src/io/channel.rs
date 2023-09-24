use std::{net::SocketAddr, rc::Rc};

use anyhow::bail;
use crossbeam_channel::Sender;

use super::{
    header::{Header, SendType, HEADER_SIZE},
    send_buffer::{SendBufferManager, SendPayload},
    sequence_buffer::SequenceBuffer,
    socket::UdpSendEvent,
    PacketType, BUFFER_SIZE, BUFFER_WINDOW_SIZE, MAX_PACKET_SIZE, RESEND_DURATION,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChannelType {
    Client,
    Server,
}

#[derive(Clone)]
pub struct Channel {
    pub mode: ChannelType,
    pub session_key: u64,
    pub addr: SocketAddr,
    pub unreliable_local_seq: u32,
    pub local_seq: u32,
    pub remote_seq: u32,
    pub send_ack: bool,
    //buffer of sent packets
    pub send_buffer: SendBufferManager,
    //tracking received packets for preventing emiting duplicate packets
    received_packets: SequenceBuffer<()>,
    sender: Sender<UdpSendEvent>,
}

impl Channel {
    pub fn new(
        addr: SocketAddr,
        session_key: u64,
        mode: ChannelType,
        sender: &Sender<UdpSendEvent>,
    ) -> Self {
        Self {
            mode,
            session_key,
            addr,
            unreliable_local_seq: 0,
            local_seq: 0,
            remote_seq: 0,
            send_ack: false,
            send_buffer: SendBufferManager::new(),
            received_packets: SequenceBuffer::with_capacity(BUFFER_SIZE),
            sender: sender.clone(),
        }
    }

    pub fn resend_reliable(&mut self, seq: u32, payload: Vec<u8>) {
        self.sender
            .send(self.make_send_event(seq, payload))
            .expect("failed sent");
        self.send_ack = false;
    }

    pub fn send_reliable(&mut self, data: Option<&[u8]>) {
        //if data.len() > MAX_PACKET_SIZE {}
        let send_buffer = self.create_send_buffer(data);
        self.sender
            .send(self.make_send_event(send_buffer.seq, send_buffer.data.to_vec()))
            .expect("failed sent");
        self.send_ack = false;
    }

    pub fn send_unreliable(&mut self, data: Option<&[u8]>) {
        let mut header = Header::new(
            self.unreliable_local_seq,
            self.session_key,
            SendType::Unreliable,
        );
        self.write_header_ack_fiels(&mut header);

        let payload = Header::create_packet(&header, data);

        self.unreliable_local_seq += 1;
        self.send_ack = false;

        self.sender
            .send(self.make_send_event(header.seq, payload))
            .unwrap();
    }

    pub fn read<'a>(&mut self, data: &'a [u8]) -> anyhow::Result<Option<&'a [u8]>> {
        if data.len() < HEADER_SIZE {
            return Ok(None);
        }

        let header = Header::read(data)?;

        //validate session key
        if header.session_key != self.session_key {
            bail!("incorrect session key");
        }

        let payload_size = data.len() - HEADER_SIZE;

        match header.packet_type {
            PacketType::PayloadReliable => {
                //always send ack even if its a duplicate
                self.send_ack = true;
                let mut new_packet = false;

                //always mark the acks
                self.mark_sent_packets(header.ack, header.ack_bits);

                //if the sequence was not registered yet its a new packet
                if self.update_remote_seq(header.seq) || self.received_packets.is_none(header.seq) {
                    //NOTE: packet is new and we dont have to check if its a duplicate
                    new_packet = true;
                }

                if new_packet {
                    self.received_packets.insert(header.seq, ());
                    if payload_size > 0 {
                        return Ok(Some(&data[HEADER_SIZE..data.len()]));
                    }
                }
            }
            PacketType::PayloadUnreliable => {
                self.mark_sent_packets(header.ack, header.ack_bits);
                if payload_size > 0 {
                    return Ok(Some(&data[HEADER_SIZE..data.len()]));
                }
            }
            _ => {}
        }

        Ok(None)
    }

    fn update_remote_seq(&mut self, remote_seq: u32) -> bool {
        if remote_seq > self.remote_seq {
            /*
             We have to maintain a sliding window of active packets thats lesser
             than the sequence buffer size so we can see which packets we received and
             prevent duplicate 'Receive' events
            */
            //WARN: this is safe until we use sequence numbers as u32. If we switch to u16 we'll get overflow
            let diff = remote_seq - self.remote_seq;
            let start = self.remote_seq - BUFFER_WINDOW_SIZE;
            for i in 0..diff {
                self.received_packets.remove(start + i);
            }

            //update to the new remote sequence
            self.remote_seq = remote_seq;

            return true;
        }

        false
    }

    pub fn write_header_ack_fiels(&self, header: &mut Header) {
        header.ack = self.remote_seq;
        header.ack_bits = self.generate_ack_field();
    }

    pub fn create_send_buffer(&mut self, data: Option<&[u8]>) -> Rc<SendPayload> {
        let mut header = Header::new(self.local_seq, self.session_key, SendType::Reliable);
        self.write_header_ack_fiels(&mut header);

        let payload = Header::create_packet(&header, data);
        let send_payload = self.send_buffer.push_send_buffer(self.local_seq, &payload);

        self.send_ack = false;
        self.local_seq += 1;

        send_payload
    }

    pub fn get_redelivery_packet(&mut self) -> Vec<Rc<SendPayload>> {
        let mut packets = Vec::new();
        if self.local_seq > 0 {
            let mut current_seq = self.local_seq - 1;

            while let Some(send_buffer) = self.send_buffer.buffers.get_mut(current_seq) {
                if let Some(sent_at) = send_buffer.sent_at {
                    if sent_at.elapsed() > RESEND_DURATION {
                        packets.push(send_buffer.payload.clone());
                        send_buffer.sent_at = None;
                    }
                }
                if current_seq > 0 {
                    current_seq -= 1;
                } else {
                    break;
                }
            }
        }

        packets
    }

    pub fn is_duplicate(&self, remote_seq: u32) {}

    pub fn mark_sent_packets(&mut self, ack: u32, ack_bitfield: u32) {
        self.send_buffer.mark_sent_packets(ack, ack_bitfield)
    }

    //least significant bit is the remote_seq - 1 value
    pub fn generate_ack_field(&self) -> u32 {
        self.send_buffer.generate_ack_field(self.remote_seq)
    }

    fn make_send_event(&self, seq: u32, payload: Vec<u8>) -> UdpSendEvent {
        match self.mode {
            ChannelType::Client => UdpSendEvent::ClientTracking(payload, seq),
            ChannelType::Server => UdpSendEvent::ServerTracking(payload, self.addr, seq),
        }
    }
}

#[cfg(test)]
mod tests {
    use bit_field::BitField;

    fn test_channel() -> Channel {
        let (t1, t2) = crossbeam_channel::unbounded();
        Channel::new(
            "127.0.0.1:21344".parse().unwrap(),
            0, //TODO
            ChannelType::Server,
            &t1,
        )
    }

    use super::*;
    #[test]
    fn marking_received_bitfields() {
        let mut channel = test_channel();
        channel.local_seq = 50;
        channel.remote_seq = 70;

        //WARN: if seq ever gets converted to u16 we need to test for appropriately for it
        let mut ack_bitfield = 0;
        ack_bitfield.set_bit(0, true);
        ack_bitfield.set_bit(1, true);
        ack_bitfield.set_bit(15, true);
        ack_bitfield.set_bit(31, true);

        channel.mark_sent_packets(50, ack_bitfield);

        assert!(*channel.send_buffer.received_acks.get(50).unwrap());
        assert!(*channel.send_buffer.received_acks.get(49).unwrap());
        assert!(*channel.send_buffer.received_acks.get(48).unwrap());
        assert!(*channel.send_buffer.received_acks.get(34).unwrap());
        assert!(*channel.send_buffer.received_acks.get(18).unwrap());
    }

    #[test]
    fn generating_received_bitfields() {
        let mut channel = test_channel();
        channel.local_seq = 50;
        channel.remote_seq = 70;

        let prev_remote_seq = channel.remote_seq - 1;
        channel
            .send_buffer
            .received_acks
            .insert(prev_remote_seq, true);
        channel
            .send_buffer
            .received_acks
            .insert(prev_remote_seq - 1, true);
        channel
            .send_buffer
            .received_acks
            .insert(prev_remote_seq - 15, true);
        channel
            .send_buffer
            .received_acks
            .insert(prev_remote_seq - 31, true);

        let mut ack_bitfield = 0;
        ack_bitfield.set_bit(0, true);
        ack_bitfield.set_bit(1, true);
        ack_bitfield.set_bit(15, true);
        ack_bitfield.set_bit(31, true);

        assert_eq!(channel.generate_ack_field(), ack_bitfield);
    }
}
