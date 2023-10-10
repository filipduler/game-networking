use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use bit_field::BitField;
use log::warn;

use crate::net::{sequence::SequenceBuffer, BUFFER_SIZE};

use super::{rtt_tracker::RttTracker, Bytes, BUFFER_WINDOW_SIZE};

const SEND_TIMEOUT: Duration = Duration::from_secs(3);

pub struct SendBuffer {
    pub payload: Rc<SendPayload>,
    pub sent_at: Option<Instant>,
}

pub struct SendPayload {
    pub seq: u16,
    //stores just the data without the header
    pub buffer: Bytes,
    pub frag: bool,
}

pub struct ReceivedAck {
    pub acked: bool,
    pub packet_created_at: Instant,
}

pub struct SendBufferManager {
    pub buffers: SequenceBuffer<SendBuffer>,
    pub received_acks: SequenceBuffer<ReceivedAck>,
    pub trr_tracker: RttTracker,
}

impl SendBufferManager {
    pub fn new() -> Self {
        SendBufferManager {
            buffers: SequenceBuffer::with_size(BUFFER_SIZE),
            received_acks: SequenceBuffer::with_size(BUFFER_SIZE),
            trr_tracker: RttTracker::new(),
        }
    }

    pub fn mark_sent(&mut self, seq: u16, sent_at: Instant) {
        if let Some(buffer) = self.buffers.get_mut(seq) {
            buffer.sent_at = Some(sent_at);
        }
    }

    pub fn push_send_buffer(&mut self, seq: u16, data: &[u8], frag: bool) -> Rc<SendPayload> {
        let send_buffer = SendBuffer {
            payload: Rc::new(SendPayload {
                seq,
                buffer: data.to_vec(),
                frag,
            }),
            sent_at: None,
        };

        let payload = send_buffer.payload.clone();

        self.received_acks.insert(
            seq,
            ReceivedAck {
                acked: false,
                packet_created_at: Instant::now(),
            },
        );
        self.buffers.insert(seq, send_buffer);

        payload
    }

    pub fn mark_acked_packets(&mut self, ack: u16, ack_bitfield: u32, received_at: &Instant) {
        //only record the latest one..
        self.ack_packet(ack, Some(received_at));

        //if its 0 that means nothing is acked..
        if ack_bitfield > 0 {
            for bit_pos in 0..32_u16 {
                if ack_bitfield.get_bit(bit_pos as usize) {
                    let seq = ack.wrapping_sub(bit_pos).wrapping_sub(1);
                    self.ack_packet(seq, None);
                }
            }
        }
    }

    fn ack_packet(&mut self, ack: u16, received_at: Option<&Instant>) {
        if let Some(received_at) = received_at {
            if let Some(buffer) = self.buffers.take(ack) {
                if let Some(sent_at) = buffer.sent_at {
                    self.trr_tracker.record_rtt(sent_at, *received_at);
                }
            }
        } else {
            self.buffers.remove(ack);
        }

        //this should be set
        if let Some(received_ack) = self.received_acks.get_mut(ack) {
            received_ack.acked = true;
        }
    }

    //least significant bit is the remote_seq - 1 value
    pub fn generate_ack_field(&self, remote_seq: u16) -> u32 {
        let mut ack_bitfield = 0;

        let mut seq = remote_seq.wrapping_sub(1);
        for pos in 0..32 {
            if let Some(value) = self.received_acks.get(seq) {
                if value.acked {
                    ack_bitfield.set_bit(pos, true);
                }
            }
            seq = seq.wrapping_sub(1);
        }

        ack_bitfield
    }

    pub fn get_redelivery_packet(
        &mut self,
        local_seq: u16,
        marked_packets: &mut Vec<Rc<SendPayload>>,
    ) {
        //start at the last sent packet
        let mut current_seq = local_seq;

        //loop through all items in the current window
        for i in 0..BUFFER_WINDOW_SIZE {
            if let Some(received_ack) = self.received_acks.get(current_seq) {
                //if the current packet timed out we can safely finish checking older ones because they expired too
                if received_ack.packet_created_at.elapsed() > SEND_TIMEOUT {
                    break;
                }

                if !received_ack.acked {
                    if let Some(send_buffer) = self.buffers.get_mut(current_seq) {
                        //we're only interested in packets that were sent already
                        if let Some(sent_at) = send_buffer.sent_at {
                            if sent_at.elapsed() > self.trr_tracker.recommended_max_rtt() {
                                //requeue the item
                                marked_packets.push(send_buffer.payload.clone());

                                //mark it as not sent again
                                send_buffer.sent_at = None;
                            }
                        }
                    }
                }
            }

            current_seq = current_seq.wrapping_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use bit_field::BitField;

    use crate::net::rtt_tracker::MAX_RTT;

    use super::*;

    #[test]
    fn redelivery_packets_timeout() {
        let mut send_buffer = SendBufferManager::new();
        let mut packets = Vec::new();
        let d = &[0];

        send_buffer.push_send_buffer(0, d, false);
        send_buffer.mark_sent(0, Instant::now());
        send_buffer.push_send_buffer(1, d, false);
        send_buffer.mark_sent(1, Instant::now());
        thread::sleep(SEND_TIMEOUT);

        send_buffer.push_send_buffer(2, d, false);
        send_buffer.mark_sent(2, Instant::now() - MAX_RTT);
        send_buffer.push_send_buffer(3, d, false);
        send_buffer.mark_sent(3, Instant::now() - MAX_RTT);
        send_buffer.push_send_buffer(4, d, false);
        send_buffer.mark_sent(4, Instant::now() - MAX_RTT);

        //because the enough time for redelivery hasn't passed we expect 0 redelivery packets
        send_buffer.get_redelivery_packet(4, &mut packets);
        assert_eq!(packets.len(), 3);
    }

    #[test]
    fn redelivery_packets() {
        let mut send_buffer = SendBufferManager::new();
        let mut packets = Vec::new();
        let d = &[0];

        send_buffer.push_send_buffer(0, d, false);
        send_buffer.push_send_buffer(1, d, false);
        send_buffer.push_send_buffer(2, d, false);
        send_buffer.push_send_buffer(3, d, false);
        send_buffer.push_send_buffer(4, d, false);
        send_buffer.push_send_buffer(5, d, false);
        send_buffer.mark_sent(0, Instant::now());
        send_buffer.mark_sent(1, Instant::now());
        send_buffer.mark_sent(2, Instant::now());
        send_buffer.mark_sent(3, Instant::now());
        send_buffer.mark_sent(4, Instant::now());
        //we're not marking sequence 5, so we don't expect it in the redelivery list

        send_buffer.mark_acked_packets(2, 0, &Instant::now());
        send_buffer.mark_acked_packets(3, 0, &Instant::now());
        send_buffer.mark_acked_packets(4, 0, &Instant::now());

        //because the enough time for redelivery hasn't passed we expect 0 redelivery packets
        send_buffer.get_redelivery_packet(6, &mut packets);
        assert_eq!(packets.len(), 0);

        //sleep for the max RTT so they become available
        thread::sleep(MAX_RTT);

        send_buffer.get_redelivery_packet(6, &mut packets);
        assert_eq!(packets.len(), 2);
        assert_eq!(packets[0].seq, 1);
        assert_eq!(packets[1].seq, 0);
    }

    #[test]
    fn marking_received_bitfields() {
        let mut send_buffer = SendBufferManager::new();

        let mut ack_bitfield = 0;
        ack_bitfield.set_bit(0, true);
        ack_bitfield.set_bit(1, true);
        ack_bitfield.set_bit(15, true);
        ack_bitfield.set_bit(31, true);

        //prepare send buffers
        let d = &[0];

        let mut seq = 5;
        for i in 0..33 {
            send_buffer.push_send_buffer(seq, d, false);

            seq = seq.wrapping_sub(1);
        }

        send_buffer.mark_acked_packets(5, ack_bitfield, &Instant::now());

        assert!(
            send_buffer
                .received_acks
                .get(4_u16.wrapping_sub(0))
                .unwrap()
                .acked
        );
        assert!(
            send_buffer
                .received_acks
                .get(4_u16.wrapping_sub(1))
                .unwrap()
                .acked
        );
        assert!(
            send_buffer
                .received_acks
                .get(4_u16.wrapping_sub(15))
                .unwrap()
                .acked
        );
        assert!(
            send_buffer
                .received_acks
                .get(4_u16.wrapping_sub(31))
                .unwrap()
                .acked
        );
    }

    #[test]
    fn generating_received_bitfields() {
        let mut send_buffer = SendBufferManager::new();
        let remote_seq = 5_u16;

        let prev_remote_seq = remote_seq - 1;
        send_buffer.received_acks.insert(
            prev_remote_seq.wrapping_sub(0),
            ReceivedAck {
                acked: true,
                packet_created_at: Instant::now(),
            },
        );
        send_buffer.received_acks.insert(
            prev_remote_seq.wrapping_sub(1),
            ReceivedAck {
                acked: true,
                packet_created_at: Instant::now(),
            },
        );
        send_buffer.received_acks.insert(
            prev_remote_seq.wrapping_sub(15),
            ReceivedAck {
                acked: true,
                packet_created_at: Instant::now(),
            },
        );
        send_buffer.received_acks.insert(
            prev_remote_seq.wrapping_sub(31),
            ReceivedAck {
                acked: true,
                packet_created_at: Instant::now(),
            },
        );

        let mut ack_bitfield = 0;
        ack_bitfield.set_bit(0, true);
        ack_bitfield.set_bit(1, true);
        ack_bitfield.set_bit(15, true);
        ack_bitfield.set_bit(31, true);

        assert_eq!(send_buffer.generate_ack_field(remote_seq), ack_bitfield);
    }
}
