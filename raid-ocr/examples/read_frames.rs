//! Run a recorded raid-frame capture through the live detection pipeline.
//!
//! The library exists apart from Tauri exactly so screenshots can replay the
//! same code the overlay runs. Point it at a PNG of the capture region and
//! describe the grid:
//!
//! ```text
//! cargo run -p baras-raid-ocr --example read_frames -- capture.png <cols> <rows> [x y w h]
//! ```
//!
//! `x y w h` restrict the grid to a sub-region of the image; without them the
//! whole image is split. Requires the recognition model the app downloads on
//! first use.

use baras_overlay::capture::CapturedImage;

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(path), Some(cols), Some(rows)) = (args.next(), parse(&mut args), parse(&mut args))
    else {
        eprintln!("usage: read_frames <capture.png> <cols> <rows> [x y w h]");
        std::process::exit(2);
    };

    if !baras_raid_ocr::engine::model_is_present() {
        eprintln!("recognition model missing; run the app once to download it");
        std::process::exit(1);
    }

    let image = load_png(&path);
    let (rx, ry) = (parse(&mut args).unwrap_or(0), parse(&mut args).unwrap_or(0));
    let rw = parse(&mut args).unwrap_or(image.width as i32 - rx);
    let rh = parse(&mut args).unwrap_or(image.height as i32 - ry);

    let (cell_w, cell_h) = (rw / cols, rh / rows);
    let slots: Vec<(u8, i32, i32, u32, u32)> = (0..cols * rows)
        .map(|slot| {
            // Column-major, matching the overlay's slot numbering.
            let (col, row) = (slot / rows, slot % rows);
            (
                slot as u8,
                rx + col * cell_w,
                ry + row * cell_h,
                cell_w as u32,
                cell_h as u32,
            )
        })
        .collect();

    let mut dump = baras_raid_ocr::DebugDump::new(slots.len());
    if let Some(dump) = &dump {
        println!("dumping crops to {:?}", dump.path());
    }
    let observations = baras_raid_ocr::observe_slots_dumping(&image, &slots, dump.as_mut());
    if let Some(dump) = dump {
        dump.finish();
    }

    println!("{} of {} slots produced a reading", observations.len(), slots.len());
    for row in &observations {
        println!(
            "slot {}  name {:?}  hp {:?} {:?}",
            row.row, row.name_text, row.hp_value, row.hp_percent
        );
    }
}

fn parse(args: &mut impl Iterator<Item = String>) -> Option<i32> {
    args.next()?.parse().ok()
}

fn load_png(path: &str) -> CapturedImage {
    let decoder = png::Decoder::new(std::fs::File::open(path).expect("cannot open image"));
    let mut reader = decoder.read_info().expect("not a PNG");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("cannot decode PNG");
    buf.truncate(info.buffer_size());

    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        other => panic!("unsupported PNG colour type {other:?}"),
    };

    CapturedImage {
        width: info.width,
        height: info.height,
        rgba,
    }
}
