extern crate af_packet;
extern crate dns_parser;
extern crate nom;
extern crate num_cpus;
extern crate pktparse;

use std::env;
use std::thread;

use dns_parser::Packet;
use pktparse::{ethernet, ipv4, ip, udp};

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut settings = af_packet::rx::RingSettings::default();
    let mut fds = Vec::<i32>::new();

    //disable rx hash stuff because DNS shouldn't need it (At least over UDP)
    settings.fanout_method = af_packet::rx::PACKET_FANOUT_LB;
    settings.ring_settings.tp_feature_req_word = 0;
    settings.if_name = args[1].clone();

    for _ in 0..num_cpus::get() {
        let ring_settings = settings.clone();
        let mut ring = af_packet::rx::Ring::new(ring_settings).unwrap();
        fds.push(ring.socket.fd);
        thread::spawn(move || {
            loop {
                let mut block = ring.get_block();
                for packet in block.get_raw_packets() {
                    //think ethernet header is 82b offset
                    if let Ok((remainder, frame)) = ethernet::parse_ethernet_frame(&packet.data[82..]) {
                        if frame.ethertype == ethernet::EtherType::IPv4 {
                            if let Ok((remainder, v4)) = ipv4::parse_ipv4_header(&remainder) {
                                if v4.protocol == ip::IPProtocol::UDP {
                                    if let Ok((remainder, udp)) = udp::parse_udp_header(&remainder) {
                                        if udp.source_port == 53 || udp.dest_port == 53 {
                                            let dns = Packet::parse(&remainder);
                                            println!("{:?}", dns);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                block.mark_as_consumed();
            }
        });
    }

    let mut packets: u64 = 0;
    let mut drops: u64 = 0;

    loop {
        let mut stats: (u64, u64) = (0, 0);
        for fd in &fds {
            let ring_stats = af_packet::rx::get_rx_statistics(*fd).unwrap();
            stats.0 += ring_stats.tp_drops as u64;
            stats.1 += ring_stats.tp_packets as u64;
        }
        drops += stats.0;
        packets += stats.1;
        eprintln!(
            "{} frames received per second, {} dropped. {} total drops of {} total packets ({}%)",
            stats.1,
            stats.0,
            drops,
            packets,
            format!("{:.*}", 4, drops as f64 / packets as f64 * 100 as f64)
        );
        thread::sleep(std::time::Duration::from_secs(1));
    }
}
