use std::{net::SocketAddr, rc::Rc};

use anyhow::bail;
use crossbeam_channel::Sender;

use super::{
    fragmentation_manager::FragmentationManager,
    header::{Header, SendType, HEADER_SIZE},
    send_buffer::{SendBufferManager, SendPayload},
    sequence::{Sequence, SequenceBuffer, WindowSequenceBuffer},
    socket::UdpSendEvent,
    PacketType, BUFFER_SIZE, BUFFER_WINDOW_SIZE, FRAGMENT_SIZE, MAGIC_NUMBER_HEADER,
    RESEND_DURATION,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChannelType {
    Client,
    Server,
}

pub enum ReadPayload<'a> {
    Ref(&'a [u8]),
    Vec(Vec<u8>),
    None,
}

pub struct Channel {
    pub mode: ChannelType,
    pub session_key: u64,
    pub addr: SocketAddr,
    pub unreliable_seq: u16,
    pub local_seq: u16,
    pub remote_seq: u16,
    pub send_ack: bool,
    //buffer of sent packets
    pub send_buffer: SendBufferManager,
    //tracking received packets for preventing emiting duplicate packets
    received_packets: WindowSequenceBuffer<()>,
    sender: Sender<UdpSendEvent>,
    //fragmentation
    fragmentation: FragmentationManager,
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
            unreliable_seq: 0,
            local_seq: 0,
            remote_seq: 0,
            send_ack: false,
            send_buffer: SendBufferManager::new(),
            received_packets: WindowSequenceBuffer::with_size(BUFFER_SIZE, BUFFER_WINDOW_SIZE),
            sender: sender.clone(),
            fragmentation: FragmentationManager::new(),
        }
    }

    pub fn send_reliable(&mut self, data: &[u8]) -> anyhow::Result<()> {
        if FragmentationManager::should_fragment(data.len()) {
            let fragments = self.fragmentation.split_fragments(data);
            for chunk in &fragments.chunks {
                let payload = self.create_send_buffer(
                    chunk.data,
                    true,
                    fragments.group_id,
                    chunk.fragment_id,
                    fragments.chunk_count,
                );
                self.send(payload.seq, &payload.data)?;
            }
        } else {
            let payload = self.create_send_buffer(data, false, 0, 0, 0);
            self.send(payload.seq, &payload.data)?;
        }

        Ok(())
    }

    pub fn send_unreliable(&mut self, data: &[u8]) -> anyhow::Result<()> {
        if FragmentationManager::should_fragment(data.len()) {
            let fragments = self.fragmentation.split_fragments(data);
            for chunk in &fragments.chunks {
                let (seq, payload) = self.create_unreliable_packet(
                    chunk.data,
                    true,
                    fragments.group_id,
                    chunk.fragment_id,
                    fragments.chunk_count,
                );
                self.send(seq, data)?;
            }
        } else {
            let (seq, payload) = self.create_unreliable_packet(data, false, 0, 0, 0);
            self.send(seq, &payload)?;
        }

        Ok(())
    }

    pub fn send_empty_ack(&mut self) -> anyhow::Result<()> {
        let empty_arr = &MAGIC_NUMBER_HEADER[0..0];

        let (seq, payload) = self.create_unreliable_packet(empty_arr, false, 0, 0, 0);

        self.sender
            .send(self.make_send_event(seq, payload.to_vec()))?;

        Ok(())
    }

    pub fn send(&mut self, seq: u16, data: &[u8]) -> anyhow::Result<()> {
        self.sender.send(self.make_send_event(seq, data.to_vec()))?;
        self.send_ack = false;

        Ok(())
    }

    pub fn read<'a>(&mut self, data: &'a [u8]) -> anyhow::Result<ReadPayload<'a>> {
        if data.len() < HEADER_SIZE {
            return Ok(ReadPayload::None);
        }

        let header = Header::read(data)?;

        //validate session key
        if header.session_key != self.session_key {
            bail!("incorrect session key");
        }

        let payload_size = data.len() - header.get_header_size();

        match header.packet_type {
            PacketType::PayloadReliable | PacketType::PayloadReliableFrag => {
                let is_frag = header.packet_type.is_frag_variant();

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
                        let payload = &data[header.get_header_size()..data.len()];
                        if is_frag {
                            if self.fragmentation.insert_fragment(&header, payload)? {
                                if let Some(data) = self
                                    .fragmentation
                                    .build_fragment(header.fragment_group_id)?
                                {
                                    return Ok(ReadPayload::Vec(data));
                                }
                            }
                        } else {
                            return Ok(ReadPayload::Ref(payload));
                        }
                    }
                }
            }
            PacketType::PayloadUnreliable | PacketType::PayloadUnreliableFrag => {
                let is_frag = header.packet_type.is_frag_variant();
                //TODO: implement
                if is_frag {
                    todo!()
                }

                self.mark_sent_packets(header.ack, header.ack_bits);
                if payload_size > 0 {
                    return Ok(ReadPayload::Ref(
                        &data[header.get_header_size()..data.len()],
                    ));
                }
            }
            _ => {}
        }

        Ok(ReadPayload::None)
    }

    fn update_remote_seq(&mut self, remote_seq: u16) -> bool {
        if Sequence::is_less_than(self.remote_seq, remote_seq) {
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

    pub fn create_unreliable_packet(
        &mut self,
        data: &[u8],
        frag: bool,
        fragment_group_id: u16,
        fragment_id: u8,
        fragment_size: u8,
    ) -> (u16, Vec<u8>) {
        let mut header = Header::new(
            self.unreliable_seq,
            self.session_key,
            SendType::Unreliable,
            false,
        );
        header.fragment_group_id = fragment_group_id;
        header.fragment_id = fragment_id;
        header.fragment_size = fragment_size;

        self.write_header_ack_fiels(&mut header);

        let payload = header.create_packet(Some(data));
        let seq = self.unreliable_seq;

        Sequence::increment(&mut self.unreliable_seq);

        (seq, payload)
    }

    pub fn create_send_buffer(
        &mut self,
        data: &[u8],
        frag: bool,
        fragment_group_id: u16,
        fragment_id: u8,
        fragment_size: u8,
    ) -> Rc<SendPayload> {
        let mut header = Header::new(self.local_seq, self.session_key, SendType::Reliable, frag);
        header.fragment_group_id = fragment_group_id;
        header.fragment_id = fragment_id;
        header.fragment_size = fragment_size;

        self.write_header_ack_fiels(&mut header);

        let payload = header.create_packet(Some(data));
        let send_payload = self
            .send_buffer
            .push_send_buffer(self.local_seq, &payload, frag);

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

    pub fn mark_sent_packets(&mut self, ack: u16, ack_bitfield: u32) {
        self.send_buffer.mark_sent_packets(ack, ack_bitfield)
    }

    //least significant bit is the remote_seq - 1 value
    pub fn generate_ack_field(&self) -> u32 {
        self.send_buffer.generate_ack_field(self.remote_seq)
    }

    fn make_send_event(&self, seq: u16, payload: Vec<u8>) -> UdpSendEvent {
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
        channel.local_seq = 5;

        let mut ack_bitfield = 0;
        ack_bitfield.set_bit(0, true);
        ack_bitfield.set_bit(1, true);
        ack_bitfield.set_bit(15, true);
        ack_bitfield.set_bit(31, true);

        channel.mark_sent_packets(5, ack_bitfield);

        assert!(*channel
            .send_buffer
            .received_acks
            .get(4_u16.wrapping_sub(0))
            .unwrap());
        assert!(*channel
            .send_buffer
            .received_acks
            .get(4_u16.wrapping_sub(1))
            .unwrap());
        assert!(*channel
            .send_buffer
            .received_acks
            .get(4_u16.wrapping_sub(15))
            .unwrap());
        assert!(*channel
            .send_buffer
            .received_acks
            .get(4_u16.wrapping_sub(31))
            .unwrap());
    }

    #[test]
    fn generating_received_bitfields() {
        let mut channel = test_channel();
        channel.remote_seq = 5;

        let prev_remote_seq = channel.remote_seq - 1;
        channel
            .send_buffer
            .received_acks
            .insert(prev_remote_seq.wrapping_sub(0), true);
        channel
            .send_buffer
            .received_acks
            .insert(prev_remote_seq.wrapping_sub(1), true);
        channel
            .send_buffer
            .received_acks
            .insert(prev_remote_seq.wrapping_sub(15), true);
        channel
            .send_buffer
            .received_acks
            .insert(prev_remote_seq.wrapping_sub(31), true);

        let mut ack_bitfield = 0;
        ack_bitfield.set_bit(0, true);
        ack_bitfield.set_bit(1, true);
        ack_bitfield.set_bit(15, true);
        ack_bitfield.set_bit(31, true);

        assert_eq!(channel.generate_ack_field(), ack_bitfield);
    }
}
