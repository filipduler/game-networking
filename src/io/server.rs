use std::{
    collections::VecDeque,
    net::SocketAddr,
    sync::Arc,
    thread::{self, JoinHandle},
    time::Duration,
};

use crossbeam_channel::{Receiver, Sender};

use super::udp::run;

pub struct Server {
    addr: SocketAddr,
    udp_thread_join: JoinHandle<()>,
    packets: Arc<parking_lot::Mutex<VecDeque<(SocketAddr, Vec<u8>)>>>,
    sender: Sender<(SocketAddr, Vec<u8>)>,
}

impl Server {
    pub fn start(addr: SocketAddr) -> std::io::Result<Self> {
        let shared_packet_list = Arc::new(parking_lot::Mutex::new(VecDeque::new()));

        let (send_tx, send_rx) = crossbeam_channel::bounded(100);
        let (recv_tx, recv_rx) = crossbeam_channel::bounded(100);
        let join = thread::spawn(|| {
            if let Err(err) = run(recv_rx, send_tx) {
                println!("err {}", err);
            }
        });

        let c_shared_packet_list = shared_packet_list.clone();
        let c_send_rx = send_rx.clone();
        thread::spawn(move || {
            let sleep_duration = Duration::from_millis(2);

            loop {
                if !c_send_rx.is_empty() {
                    let mut packets = c_shared_packet_list.lock();

                    while let Ok(packet) = c_send_rx.try_recv() {
                        packets.push_front(packet)
                    }
                }

                //sleep for a little bit to queue up packets, so we dont lock too often
                thread::sleep(sleep_duration);
            }
        });

        Ok(Server {
            addr,
            udp_thread_join: join,
            packets: shared_packet_list,
            sender: recv_tx,
        })
    }
}
