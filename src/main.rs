use slide::{
    Slide,
    lz::{Config, Item},
    search_buffer::SearchBuffer,
};
use std::{
    fs::File,
    io::{BufReader, Read},
    iter,
};

fn main() {
    const CONFIG: Config = Config {
        max_buffer_len: 1 << 24,
        match_lengths: 8..usize::MAX,
    };
    let source = {
        let mut buf = vec![];
        BufReader::new(File::open("res/silesia/xml").unwrap())
            .read_to_end(&mut buf)
            .unwrap();
        buf
    };
    let end = source.len();
    let mut len = 0;

    let items = Vec::from_iter(
        SearchBuffer::<u8, { CONFIG.match_lengths.start }>::new()
            .to_items(source.iter().copied(), CONFIG)
            .inspect(|item| {
                len += item.len();
                if len % 0x10000 == 0 {
                    println!(">> {}% - ({len}/{end})", len as f64 * 100f64 / end as f64);
                }
            }),
    );
    let encoded = Vec::from_iter(
        items
            .iter()
            .flat_map(|item| postcard::to_stdvec(item).unwrap()),
    );
    len = 0;
    let items2 = Vec::from_iter(
        iter::from_fn({
            let mut bytes = encoded.as_slice();
            move || {
                if bytes.is_empty() {
                    return None;
                }
                let item;
                (item, bytes) = postcard::take_from_bytes::<Item<u8>>(bytes).unwrap();
                Some(item)
            }
        })
        .inspect(|item| {
            len += item.len();
            if len % 0x10000 == 0 {
                println!("<< {}% - ({len}/{end})", len as f64 * 100f64 / end as f64);
            }
        }),
    );
    assert_eq!(items, items2);
    let decoded = Vec::from_iter(Slide::new().from_items(items2, CONFIG));
    assert!(source == decoded);
    println!();
    println!("----------------------");
    println!(
        "{}% - ({}/{})",
        encoded.len() as f64 * 100f64 / end as f64,
        encoded.len(),
        end
    );
}
