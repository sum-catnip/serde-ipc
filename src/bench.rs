use serde_ipc::IpcNode;
use std::thread;
use std::time::Instant;

use log::debug;
use simple_logger;

fn main() {
    vec_5_500k();
    vec_5_1k();
}

fn vec_5_500k() {
    let mut srv = IpcNode::new(654321).unwrap();
    let vec = vec![11u8; 500000];
    let reader = thread::spawn(|| {
        let mut node = IpcNode::open(654321).unwrap();
        let _: Vec<u8> = node.recv().unwrap();
        let _: Vec<u8> = node.recv().unwrap();
        let _: Vec<u8> = node.recv().unwrap();
        let _: Vec<u8> = node.recv().unwrap();
        let _: Vec<u8> = node.recv().unwrap();
    });

    let now = Instant::now();

    srv.send(&vec).unwrap();
    debug!("5500kv1 done");
    srv.send(&vec).unwrap();
    debug!("5500kv2 done");
    srv.send(&vec).unwrap();
    debug!("5500kv3 done");
    srv.send(&vec).unwrap();
    debug!("5500kv4 done");
    srv.send(&vec).unwrap();
    debug!("5500kv5 done");
    reader.join().unwrap();

    println!("5 500k vecs took {:?}", now.elapsed());
}

fn vec_5_1k() {
    let mut srv = IpcNode::new(666).unwrap();
    let vec = vec![11u8; 1000];
    let reader = thread::spawn(|| {
        let mut node = IpcNode::open(666).unwrap();
        let _: Vec<u8> = node.recv().unwrap();
        let _: Vec<u8> = node.recv().unwrap();
        let _: Vec<u8> = node.recv().unwrap();
        let _: Vec<u8> = node.recv().unwrap();
        let _: Vec<u8> = node.recv().unwrap();
    });

    let now = Instant::now();

    srv.send(&vec).unwrap();
    debug!("51kv1 done");
    srv.send(&vec).unwrap();
    debug!("51kv2 done");
    srv.send(&vec).unwrap();
    debug!("51kv3 done");
    srv.send(&vec).unwrap();
    debug!("51kv4 done");
    srv.send(&vec).unwrap();
    debug!("51kv5 done");
    reader.join().unwrap();

    println!("5 1k vecs took {:?}", now.elapsed());
}
