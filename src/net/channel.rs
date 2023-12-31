use std::{
    borrow::BorrowMut,
    cell::RefCell,
    collections::VecDeque,
    net::SocketAddr,
    ops::{Deref, DerefMut},
    rc::Rc,
    sync::Arc,
    time::Instant,
};

use anyhow::bail;
use bit_field::BitField;
use crossbeam_channel::Sender;
use log::{debug, info};

use super::{
    bytes, bytes_with_header,
    fragmentation_manager::FragmentationManager,
    header::{Header, SendType, HEADER_SIZE},
    int_buffer::{self, IntBuffer},
    packets::SendEvent,
    send_buffer::{SendBufferManager, SendPayload},
    sequence::{Sequence, SequenceBuffer, WindowSequenceBuffer},
    socket::UdpSendEvent,
    Bytes, PacketType, BUFFER_SIZE, BUFFER_WINDOW_SIZE, MAGIC_NUMBER_HEADER,
};

#[derive(PartialEq, Eq)]
pub enum ChannelType {
    Client,
    Server,
}

pub enum ReadPayload {
    Single(Bytes),
    Parts(Vec<Bytes>),
    Disconnect,
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
    //tracking received packets for preventing emitting duplicate packets and generating acks
    received_packets: WindowSequenceBuffer<()>,
    //fragmentation
    reliable_fragmentation: FragmentationManager,
    unreliable_fragmentation: FragmentationManager,
}

impl Channel {
    pub fn new(addr: SocketAddr, session_key: u64, mode: ChannelType) -> Self {
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
            reliable_fragmentation: FragmentationManager::new(),
            unreliable_fragmentation: FragmentationManager::new(),
        }
    }

    pub fn send_event(
        &mut self,
        send_event: SendEvent,
        send_queue: &mut VecDeque<UdpSendEvent>,
    ) -> anyhow::Result<()> {
        match send_event {
            SendEvent::Single(mut buffer, reliable) => {
                if reliable {
                    let seq: u16 = self.create_send_buffer(&mut buffer, false, 0, 0, 0)?;
                    self.send_tracking(seq, buffer, send_queue);
                } else {
                    self.create_unreliable_packet(&mut buffer, false, 0, 0, 0);
                    self.send_non_tracking(buffer, send_queue);
                }
            }
            SendEvent::Fragmented(mut fragments, reliable) => {
                let fragments = self.reliable_fragmentation.split_fragments(fragments)?;
                for mut chunk in fragments.chunks {
                    if reliable {
                        let seq: u16 = self.create_send_buffer(
                            &mut chunk.buffer,
                            true,
                            fragments.group_id,
                            chunk.fragment_id,
                            fragments.chunk_count,
                        )?;
                        self.send_tracking(seq, chunk.buffer, send_queue);
                    } else {
                        self.create_unreliable_packet(
                            &mut chunk.buffer,
                            true,
                            fragments.group_id,
                            chunk.fragment_id,
                            fragments.chunk_count,
                        );
                        self.send_non_tracking(chunk.buffer, send_queue);
                    }
                }
            }
            SendEvent::Disconnect => {
                //send three disconnect packets
                for _ in 0..3 {
                    let mut header = Header::new_disconnect(self.unreliable_seq, self.session_key);
                    let mut buffer = bytes_with_header!(HEADER_SIZE);

                    let mut int_buffer = IntBuffer::new_at(4);
                    header.write(&mut buffer, &mut int_buffer)?;

                    Sequence::increment(&mut self.unreliable_seq);

                    self.send_non_tracking(buffer, send_queue);
                }
            }
        };

        Ok(())
    }

    pub fn send_empty_ack(
        &mut self,
        send_queue: &mut VecDeque<UdpSendEvent>,
    ) -> anyhow::Result<()> {
        let mut int_buffer = IntBuffer::new_at(4);
        let mut buffer = bytes_with_header!(HEADER_SIZE);

        self.create_unreliable_packet(&mut buffer, false, 0, 0, 0);

        self.send_non_tracking(buffer, send_queue);

        Ok(())
    }

    fn send_tracking(&mut self, seq: u16, buffer: Bytes, send_queue: &mut VecDeque<UdpSendEvent>) {
        let header = Header::read(&buffer[4..]).unwrap();

        send_queue.push_front(match self.mode {
            ChannelType::Client => UdpSendEvent::ClientTracking(buffer, seq),
            ChannelType::Server => UdpSendEvent::ServerTracking(buffer, self.addr, seq),
        });
        self.send_ack = false;
    }

    fn send_non_tracking(&mut self, buffer: Bytes, send_queue: &mut VecDeque<UdpSendEvent>) {
        send_queue.push_front(match self.mode {
            ChannelType::Client => UdpSendEvent::Client(buffer),
            ChannelType::Server => UdpSendEvent::Server(buffer, self.addr),
        });
        self.send_ack = false;
    }

    pub fn read(
        &mut self,
        mut buffer: Bytes,
        received_at: &Instant,
    ) -> anyhow::Result<ReadPayload> {
        let header = Header::read(&buffer)?;

        //validate session key
        if header.session_key != self.session_key {
            bail!("incorrect session key");
        }

        //client requested a disconnect
        if header.packet_type == PacketType::Disconnect {
            return Ok(ReadPayload::Disconnect);
        }

        //remove the header data from the buffer
        _ = buffer.drain(0..header.get_header_size());

        match header.packet_type {
            PacketType::PayloadReliable | PacketType::PayloadReliableFrag => {
                //always send ack even if its a duplicate
                self.send_ack = true;
                let mut new_packet = false;

                //always mark the acks
                self.mark_acked_packets(header.ack, header.ack_bits, received_at);

                //if the sequence was not registered yet its a new packet
                if self.update_remote_seq(header.seq) || self.received_packets.is_none(header.seq) {
                    //NOTE: packet is new and we don't have to check if its a duplicate
                    new_packet = true;
                }

                if new_packet {
                    self.received_packets.insert(header.seq, ());

                    if !buffer.is_empty() {
                        if header.packet_type.is_frag_variant() {
                            if self
                                .reliable_fragmentation
                                .insert_fragment(&header, buffer)?
                            {
                                info!(
                                    "finished constructing new fragment with id {}",
                                    header.fragment_group_id
                                );
                                return Ok(ReadPayload::Parts(
                                    self.reliable_fragmentation
                                        .assemble(header.fragment_group_id)?,
                                ));
                            }
                        } else {
                            return Ok(ReadPayload::Single(buffer));
                        }
                    }
                }
            }
            PacketType::PayloadUnreliable | PacketType::PayloadUnreliableFrag => {
                self.mark_acked_packets(header.ack, header.ack_bits, received_at);

                if !buffer.is_empty() {
                    if header.packet_type.is_frag_variant() {
                        if self
                            .unreliable_fragmentation
                            .insert_fragment(&header, buffer)?
                        {
                            return Ok(ReadPayload::Parts(
                                self.unreliable_fragmentation
                                    .assemble(header.fragment_group_id)?,
                            ));
                        }
                    } else {
                        return Ok(ReadPayload::Single(buffer));
                    }
                }
            }
            _ => {}
        }

        Ok(ReadPayload::None)
    }

    pub fn update(
        &mut self,
        marked_packets: &mut Vec<Rc<SendPayload>>,
        send_queue: &mut VecDeque<UdpSendEvent>,
    ) -> anyhow::Result<()> {
        self.send_buffer
            .get_redelivery_packet(self.local_seq, marked_packets);

        while let Some(packet) = marked_packets.pop() {
            let mut header = packet.original_header;
            self.write_header_ack_fields(&mut header);

            let mut int_buffer = IntBuffer::new_at(4);
            let mut buffer = bytes_with_header!(header.get_header_size() + packet.buffer.len());

            header.write(&mut buffer, &mut int_buffer)?;
            int_buffer.write_slice(&packet.buffer, &mut buffer);

            self.send_tracking(header.seq, buffer, send_queue);
        }

        if self.send_ack {
            self.send_empty_ack(send_queue)?;
        }

        Ok(())
    }

    fn update_remote_seq(&mut self, remote_seq: u16) -> bool {
        if Sequence::is_less_than(self.remote_seq, remote_seq) {
            //update to the new remote sequence
            self.remote_seq = remote_seq;

            return true;
        }

        false
    }

    fn write_header_ack_fields(&self, header: &mut Header) {
        header.ack = self.remote_seq;
        header.ack_bits = self.generate_ack_field();
    }

    pub fn create_unreliable_packet(
        &mut self,
        buffer: &mut Bytes,
        frag: bool,
        fragment_group_id: u16,
        fragment_id: u8,
        fragment_size: u8,
    ) -> anyhow::Result<()> {
        let mut header = Header::new(
            self.unreliable_seq,
            self.session_key,
            SendType::Unreliable,
            false,
        );
        header.fragment_group_id = fragment_group_id;
        header.fragment_id = fragment_id;
        header.fragment_size = fragment_size;

        self.write_header_ack_fields(&mut header);

        let mut int_buffer = IntBuffer::new_at(4);
        header.write(buffer, &mut int_buffer)?;

        Sequence::increment(&mut self.unreliable_seq);

        Ok(())
    }

    pub fn create_send_buffer(
        &mut self,
        buffer: &mut Bytes,
        frag: bool,
        fragment_group_id: u16,
        fragment_id: u8,
        fragment_size: u8,
    ) -> anyhow::Result<u16> {
        let mut header = Header::new(self.local_seq, self.session_key, SendType::Reliable, frag);
        header.fragment_group_id = fragment_group_id;
        header.fragment_id = fragment_id;
        header.fragment_size = fragment_size;

        self.write_header_ack_fields(&mut header);

        let mut int_buffer = IntBuffer::new_at(4);
        header.write(buffer, &mut int_buffer)?;

        let send_payload = self.send_buffer.push_send_buffer(
            self.local_seq,
            &buffer[4 + header.get_header_size()..], //pass the just the data
            &header,
        );

        Sequence::increment(&mut self.local_seq);

        Ok(send_payload.original_header.seq)
    }

    pub fn mark_acked_packets(&mut self, ack: u16, ack_bitfield: u32, received_at: &Instant) {
        self.send_buffer
            .mark_acked_packets(ack, ack_bitfield, received_at)
    }

    //least significant bit is the remote_seq - 1 value
    pub fn generate_ack_field(&self) -> u32 {
        let mut ack_bitfield = 0;

        let mut seq = self.remote_seq.wrapping_sub(1);
        for pos in 0..32 {
            if self.received_packets.is_some(seq) {
                ack_bitfield.set_bit(pos, true);
            }
            seq = seq.wrapping_sub(1);
        }
        ack_bitfield
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn generating_received_bitfields() {
        let mut channel = Channel::new("127.0.0.1:9090".parse().unwrap(), 0, ChannelType::Client);
        channel.remote_seq = 5;

        let prev_remote_seq = channel.remote_seq - 1;
        channel
            .received_packets
            .insert(prev_remote_seq.wrapping_sub(0), ());
        channel
            .received_packets
            .insert(prev_remote_seq.wrapping_sub(1), ());
        channel
            .received_packets
            .insert(prev_remote_seq.wrapping_sub(15), ());
        channel
            .received_packets
            .insert(prev_remote_seq.wrapping_sub(31), ());

        let mut ack_bitfield = 0;
        ack_bitfield.set_bit(0, true);
        ack_bitfield.set_bit(1, true);
        ack_bitfield.set_bit(15, true);
        ack_bitfield.set_bit(31, true);

        assert_eq!(channel.generate_ack_field(), ack_bitfield);
    }
}
