fn main() {
    let p = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let info = tracker_media::open(&p).unwrap();
    println!("duration {} index {}", info.duration, info.index.len());
    println!("index: {:?}", info.index.stamps().iter().map(|s| (s.value, s.timescale)).collect::<Vec<_>>());
    let mut r = tracker_media::FrameReader::new(&info, info.index.first().unwrap(), false).unwrap();
    let mut v = vec![];
    while let Some(f) = r.next_frame().unwrap() { v.push((f.stamp.value, f.stamp.timescale)); }
    println!("decoded {}: {:?}", v.len(), v);
}
