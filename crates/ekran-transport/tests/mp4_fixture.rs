use ekran_transport::{BoxReader, FragmentAssembler, MediaChunk};
use std::io::Cursor;

#[tokio::test]
async fn sample_frag_mp4_splits_init_and_fragments() {
    let data = include_bytes!("fixtures/sample_frag.mp4");
    let mut reader = BoxReader::new(Cursor::new(&data[..]));
    let mut asm = FragmentAssembler::new();

    let mut init_count = 0usize;
    let mut frag_count = 0usize;
    while let Some(bx) = reader.next_box().await.expect("valid box") {
        if let Some(chunk) = asm.push(bx) {
            match chunk {
                MediaChunk::Init(b) => {
                    assert!(b.len() > 32);
                    assert_eq!(&b[4..8], b"ftyp");
                    init_count += 1;
                }
                MediaChunk::Fragment(b) => {
                    assert!(b.len() > 16);
                    assert_eq!(&b[4..8], b"moof");
                    frag_count += 1;
                }
            }
        }
    }
    assert_eq!(init_count, 1);
    assert!(frag_count >= 2, "expected >=2 fragments, got {frag_count}");
}
