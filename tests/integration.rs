use serde_ipc::IpcNode;
use std::thread;
use simple_logger;

#[test]
fn test_server_smoll() {
    let _ = simple_logger::init();
    let mut srv = IpcNode::new(123456).unwrap();
    srv.send(&666u32).unwrap();
    let res: u32 = srv.recv().unwrap();
    assert_eq!(res, 666u32);
}

#[test]
fn test_server_big() {
    let _ = simple_logger::init();
    let mut srv = IpcNode::new(654321).unwrap();
    let vec = vec![11u8; 5000];
    let reader = thread::spawn(|| {
        let mut node = IpcNode::open(654321).unwrap();
        let res: Vec<u8> = node.recv().unwrap();
        assert_eq!(res, vec![11u8; 5000]);
    });
    srv.send(&vec).unwrap();
    reader.join().unwrap();
}

#[test]
fn test_server_smoll_2() {
    let _ = simple_logger::init();
    let mut srv = IpcNode::new(420).unwrap();
    let vec = vec![11u8; 10];
    let reader = thread::spawn(|| {
        let mut node = IpcNode::open(420).unwrap();
        let res1: Vec<u8> = node.recv().unwrap();
        let res2: Vec<u8> = node.recv().unwrap();
        assert_eq!(res1, vec![11u8; 10]);
        assert_eq!(res2, vec![11u8; 10]);
    });
    srv.send(&vec).unwrap();
    srv.send(&vec).unwrap();
    reader.join().unwrap();
}
