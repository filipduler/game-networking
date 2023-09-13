mod io;

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[cfg(test)]
mod tests {
    use crate::io::udp::run;

    use super::*;

    #[test]
    fn it_works() {
        let (send_tx, send_rx) = crossbeam_channel::bounded(100);
        let (recv_tx, recv_rx) = crossbeam_channel::bounded(100);

        if let Err(err) = run(recv_rx, send_tx) {
            println!("err {}", err);
        }
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
